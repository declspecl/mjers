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


@dataclass(frozen=True)
class PolicyMetadata:
    architecture: Literal["mlp-relu-v1"]
    hidden_width: int
    dataset: DatasetMetadata
    training_seeds: tuple[int, ...]
    validation_seeds: tuple[int, ...]


@dataclass(frozen=True)
class TrainedPolicy:
    network: PolicyNet
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
    network = PolicyNet(metadata.hidden_width).to(device)
    network.load_state_dict(load_file(path, device=str(device)), strict=True)
    network.eval()
    return TrainedPolicy(network, metadata, device)


def hand_seeds(seeds: NDArray[np.uint64]) -> tuple[int, ...]:
    return tuple(int(seed) for seed in unique(seeds))
