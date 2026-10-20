"""Small masked policy network and its portable checkpoint."""

from contextlib import AbstractContextManager
from dataclasses import dataclass
from pathlib import Path
from typing import Literal, Protocol

import msgspec
import numpy as np
import torch
from numpy import unique
from numpy.typing import NDArray
from safetensors import safe_open
from safetensors.torch import load_file, save_file
from torch import Tensor, from_numpy, nn

from mjers._native import ACTION_COUNT, CONTEXT_SIZE, ENCODING_VERSION, TILE_PLANES, Observation
from mjers.data import DatasetMetadata

type DeviceName = Literal["cpu", "mps", "cuda"]
type Architecture = Literal["mlp-relu-v1", "suit-cnn-relu-v1", "suit-resnet-v2"]

# Value targets are settlement units scaled to roughly unit variance.
VALUE_SCALE = 8.0


class PolicyNet(nn.Module):
    def __init__(self, hidden_width: int) -> None:
        super().__init__()
        if hidden_width <= 0:
            raise ValueError("hidden width must be positive")
        self.hidden_width: int = hidden_width
        self.input_layer: nn.Linear = nn.Linear(TILE_PLANES * 34 + CONTEXT_SIZE, hidden_width)
        self.hidden_layer: nn.Linear = nn.Linear(hidden_width, hidden_width)
        self.action_layer: nn.Linear = nn.Linear(hidden_width, ACTION_COUNT)

    def forward(self, tiles: Tensor, context: Tensor, action_mask: Tensor) -> Tensor:
        features = torch.cat((tiles.flatten(start_dim=1), context), dim=1)
        hidden = self.input_layer.forward(features).relu()
        hidden = self.hidden_layer.forward(hidden).relu()
        logits = self.action_layer.forward(hidden)
        return logits.masked_fill(~action_mask, -torch.inf)

    def heads(self, tiles: Tensor, context: Tensor, action_mask: Tensor) -> tuple[Tensor, Tensor | None]:
        return self.forward(tiles, context, action_mask), None


class SuitPolicyNet(nn.Module):
    """Share rank-pattern filters across suits; honors have no rank adjacency."""

    def __init__(self, hidden_width: int) -> None:
        super().__init__()
        if hidden_width <= 0:
            raise ValueError("hidden width must be positive")
        self.hidden_width: int = hidden_width
        self.suit_input: nn.Conv1d = nn.Conv1d(TILE_PLANES, 48, kernel_size=3, padding=1)
        self.suit_hidden: nn.Conv1d = nn.Conv1d(48, 48, kernel_size=3, padding=1)
        self.honor_input: nn.Conv1d = nn.Conv1d(TILE_PLANES, 48, kernel_size=1)
        self.honor_hidden: nn.Conv1d = nn.Conv1d(48, 48, kernel_size=1)
        self.input_layer: nn.Linear = nn.Linear(48 * 34 + CONTEXT_SIZE, hidden_width)
        self.hidden_layer: nn.Linear = nn.Linear(hidden_width, hidden_width)
        self.action_layer: nn.Linear = nn.Linear(hidden_width, ACTION_COUNT)

    def tile_features(self, tiles: Tensor) -> Tensor:
        # Separate suits into batch entries before convolution: 9m and 1p
        # must never become adjacent, while each suit uses the same filters.
        batch_size = tiles.shape[0]
        suits = tiles[:, :, :27].reshape(batch_size, TILE_PLANES, 3, 9).permute(0, 2, 1, 3)
        suits = suits.reshape(batch_size * 3, TILE_PLANES, 9)
        suits = self.suit_input.forward(suits).relu()
        suits = self.suit_hidden.forward(suits).relu()
        suits = suits.reshape(batch_size, 3, 48, 9).permute(0, 2, 1, 3).reshape(batch_size, 48, 27)
        honors = self.honor_input.forward(tiles[:, :, 27:]).relu()
        honors = self.honor_hidden.forward(honors).relu()
        return torch.cat((suits, honors), dim=2)

    def forward(self, tiles: Tensor, context: Tensor, action_mask: Tensor) -> Tensor:
        features = torch.cat((self.tile_features(tiles).flatten(start_dim=1), context), dim=1)
        hidden = self.input_layer.forward(features).relu()
        hidden = self.hidden_layer.forward(hidden).relu()
        logits = self.action_layer.forward(hidden)
        return logits.masked_fill(~action_mask, -torch.inf)

    def heads(self, tiles: Tensor, context: Tensor, action_mask: Tensor) -> tuple[Tensor, Tensor | None]:
        return self.forward(tiles, context, action_mask), None


TILE_ACTIONS = 102
OFFERED_PLANE = 48
RESIDUAL_BLOCKS = 6


class TileBlock(nn.Module):
    """A residual block over tiles: rank-adjacent within each suit, pointwise for
    honors, with a whole-hand pooled bias so every tile sees the others.

    Suits are a [batch, channels, 3, 9] grid; (1, 3) kernels share filters
    across suits and never let 9m touch 1p.
    """

    def __init__(self, channels: int) -> None:
        super().__init__()
        self.suit_first: nn.Conv2d = nn.Conv2d(channels, channels, kernel_size=(1, 3), padding=(0, 1))
        self.suit_second: nn.Conv2d = nn.Conv2d(channels, channels, kernel_size=(1, 3), padding=(0, 1))
        self.honor_first: nn.Conv1d = nn.Conv1d(channels, channels, kernel_size=1)
        self.honor_second: nn.Conv1d = nn.Conv1d(channels, channels, kernel_size=1)
        self.pooled: nn.Linear = nn.Linear(2 * channels, channels)
        # Each block starts as the identity, so depth does not disturb early training.
        for layer in (self.suit_second, self.honor_second):
            nn.init.zeros_(layer.weight)
            if layer.bias is not None:
                nn.init.zeros_(layer.bias)

    def forward(self, suits: Tensor, honors: Tensor) -> tuple[Tensor, Tensor]:
        active_suits, active_honors = suits.relu(), honors.relu()
        bias = self.pooled.forward(pool(active_suits, active_honors))
        hidden_suits = (self.suit_first.forward(active_suits) + bias[:, :, None, None]).relu()
        hidden_honors = (self.honor_first.forward(active_honors) + bias[:, :, None]).relu()
        return suits + self.suit_second.forward(hidden_suits), honors + self.honor_second.forward(hidden_honors)


def pool(suits: Tensor, honors: Tensor) -> Tensor:
    """Mean and maximum of each channel over all 34 tiles."""
    flat = torch.cat((suits.flatten(start_dim=2), honors), dim=2)
    return torch.cat((flat.mean(dim=2), flat.amax(dim=2)), dim=1)


class ResidualPolicyNet(nn.Module):
    """Residual suit-aware tile features with per-tile action logits and a value head.

    Discards and kongs score each tile from that tile's features, so the same
    pattern earns the same logit in every suit. Claims, wins and passing read
    pooled features, the offered tile's features, and the context. The value
    head predicts the acting seat's settlement, scaled by `VALUE_SCALE`.
    """

    def __init__(self, hidden_width: int) -> None:
        super().__init__()
        if hidden_width <= 0:
            raise ValueError("hidden width must be positive")
        self.hidden_width: int = hidden_width
        self.stem: nn.Conv1d = nn.Conv1d(TILE_PLANES + CONTEXT_SIZE, hidden_width, kernel_size=1)
        self.blocks: nn.ModuleList = nn.ModuleList(TileBlock(hidden_width) for _ in range(RESIDUAL_BLOCKS))
        self.tile_actions: nn.Conv1d = nn.Conv1d(hidden_width, TILE_ACTIONS // 34, kernel_size=1)
        self.summary: nn.Linear = nn.Linear(3 * hidden_width + CONTEXT_SIZE, hidden_width)
        self.other_actions: nn.Linear = nn.Linear(hidden_width, ACTION_COUNT - TILE_ACTIONS)
        self.value_layer: nn.Linear = nn.Linear(hidden_width, 1)

    def heads(self, tiles: Tensor, context: Tensor, action_mask: Tensor) -> tuple[Tensor, Tensor]:
        batch_size = tiles.shape[0]
        broadcast = context.unsqueeze(2).expand(-1, -1, tiles.shape[2])
        features = self.stem.forward(torch.cat((tiles, broadcast), dim=1))
        suits = features[:, :, :27].reshape(batch_size, self.hidden_width, 3, 9)
        honors = features[:, :, 27:]
        for block in self.blocks:
            assert isinstance(block, TileBlock)
            suits, honors = block.forward(suits, honors)
        active = torch.cat((suits.flatten(start_dim=2), honors), dim=2).relu()
        tile_logits = self.tile_actions.forward(active).flatten(start_dim=1)
        offered = (active * tiles[:, OFFERED_PLANE].unsqueeze(1)).sum(dim=2)
        summary = torch.cat((active.mean(dim=2), active.amax(dim=2), offered, context), dim=1)
        hidden = self.summary.forward(summary).relu()
        logits = torch.cat((tile_logits, self.other_actions.forward(hidden)), dim=1)
        return logits.masked_fill(~action_mask, -torch.inf), self.value_layer.forward(hidden).squeeze(1)

    def forward(self, tiles: Tensor, context: Tensor, action_mask: Tensor) -> Tensor:
        return self.heads(tiles, context, action_mask)[0]


type PolicyNetwork = PolicyNet | SuitPolicyNet | ResidualPolicyNet


def build_network(architecture: Architecture, hidden_width: int) -> PolicyNetwork:
    if architecture == "mlp-relu-v1":
        return PolicyNet(hidden_width)
    if architecture == "suit-cnn-relu-v1":
        return SuitPolicyNet(hidden_width)
    return ResidualPolicyNet(hidden_width)


@dataclass(frozen=True)
class Reinforcement:
    """One reinforcement stage: hands `start_seed..start_seed + hands` against `field_json`."""

    field_json: str
    start_seed: int
    hands: int

    def seeds(self) -> range:
        return range(self.start_seed, self.start_seed + self.hands)


@dataclass(frozen=True)
class PolicyMetadata:
    architecture: Architecture
    hidden_width: int
    dataset: DatasetMetadata
    training_seeds: tuple[int, ...]
    validation_seeds: tuple[int, ...]
    # Empty for imitation-only policies; the default keeps older checkpoints loadable.
    reinforcement: tuple[Reinforcement, ...] = ()


@dataclass(frozen=True)
class TrainedPolicy:
    network: PolicyNetwork
    metadata: PolicyMetadata
    device: torch.device

    def act(self, observation: Observation) -> int:
        tiles = from_numpy(observation.tiles).unsqueeze(0).to(self.device)
        context = from_numpy(observation.context).unsqueeze(0).to(self.device)
        action_mask = from_numpy(observation.action_mask).unsqueeze(0).to(self.device)
        with torch.inference_mode():
            logits = self.network.forward(tiles, context, action_mask)
            return int(logits.argmax(dim=1).item())


def save_policy(path: Path, policy: TrainedPolicy) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    weights = {name: tensor.detach().cpu().contiguous() for name, tensor in policy.network.state_dict().items()}
    metadata = {"mjers": msgspec.json.encode(policy.metadata).decode()}
    save_file(weights, path, metadata=metadata)


class CheckpointHeader(Protocol):
    """The metadata API missing from safetensors' generated Python stubs."""

    def metadata(self) -> dict[str, str] | None: ...


def load_policy(path: Path, device: torch.device) -> TrainedPolicy:
    handle: AbstractContextManager[CheckpointHeader] = safe_open(path, framework="pt", device="cpu")
    with handle as checkpoint:
        header = checkpoint.metadata()
        if header is None or "mjers" not in header:
            raise ValueError("checkpoint is missing mjers metadata")
        metadata = msgspec.json.decode(header["mjers"], type=PolicyMetadata)
    if metadata.dataset.encoding_version != ENCODING_VERSION:
        raise ValueError("checkpoint encoding is incompatible")
    network = build_network(metadata.architecture, metadata.hidden_width).to(device)
    network.load_state_dict(load_file(path, device=str(device)), strict=True)
    network.eval()
    return TrainedPolicy(network, metadata, device)


def hand_seeds(seeds: NDArray[np.uint64]) -> tuple[int, ...]:
    return tuple(int(seed) for seed in unique(seeds))
