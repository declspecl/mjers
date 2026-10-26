"""Train a masked policy by imitation on disjoint hand-seed datasets.

With `--soft-teacher`, the policy instead learns a network teacher's full move
probabilities on the dataset's positions (distillation), so close calls the
argmax labels flatten keep their margins.
"""

import argparse
import math
from collections.abc import Callable
from dataclasses import dataclass, replace
from pathlib import Path
from typing import Literal

import msgspec
import torch
from torch import Tensor, from_numpy, manual_seed
from torch.autograd import backward
from torch.nn import functional as F
from torch.optim import Adam, Optimizer

from mjers._native import TILE_PLANES
from mjers.augment import random_relabeling, relabel_actions, relabel_mask, relabel_tiles
from mjers.data import DecisionDataset, load_dataset, load_datasets
from mjers.model import (
    RESIDUAL_BLOCKS,
    VALUE_SCALE,
    Architecture,
    DeviceName,
    PolicyMetadata,
    PolicyNetwork,
    ResidualPolicyNet,
    TrainedPolicy,
    build_network,
    hand_seeds,
    load_policy,
    save_policy,
)

type Augmentation = Literal["none", "symmetry"]


@dataclass(frozen=True)
class TrainingConfig:
    architecture: Architecture
    hidden_width: int
    epochs: int
    batch_size: int
    learning_rate: float
    seed: int
    device: DeviceName
    threads: int
    augment: Augmentation
    value_weight: float
    # A policy checkpoint whose move probabilities replace the dataset's labels.
    soft_teacher: str | None

    def __post_init__(self) -> None:
        if min(self.hidden_width, self.epochs, self.batch_size, self.threads) <= 0:
            raise ValueError("width, epochs, batch size, and thread count must be positive")
        if not math.isfinite(self.learning_rate) or self.learning_rate <= 0:
            raise ValueError("learning rate must be positive and finite")
        if not math.isfinite(self.value_weight) or self.value_weight < 0:
            raise ValueError("value weight must be nonnegative and finite")
        if self.value_weight > 0 and self.architecture != "suit-resnet-v2":
            raise ValueError("only suit-resnet-v2 has a value head")


@dataclass(frozen=True)
class PolicyBatch:
    tiles: Tensor
    context: Tensor
    action_mask: Tensor
    actions: Tensor
    returns: Tensor
    # Move probabilities to learn instead of `actions` (distillation).
    targets: Tensor | None = None

    @classmethod
    def from_dataset(cls, dataset: DecisionDataset, device: torch.device) -> "PolicyBatch":
        return cls(
            from_numpy(dataset.tiles).to(device),
            from_numpy(dataset.context).to(device),
            from_numpy(dataset.action_mask).to(device),
            from_numpy(dataset.actions).to(device),
            from_numpy(dataset.returns).to(device),
        )

    def map(self, function: Callable[[Tensor], Tensor]) -> "PolicyBatch":
        targets = None if self.targets is None else function(self.targets)
        return PolicyBatch(
            function(self.tiles),
            function(self.context),
            function(self.action_mask),
            function(self.actions),
            function(self.returns),
            targets,
        )

    def select(self, indices: Tensor) -> "PolicyBatch":
        return self.map(lambda tensor: tensor[indices])

    def rows(self, start: int, end: int) -> "PolicyBatch":
        return self.map(lambda tensor: tensor[start:end])

    def to(self, device: torch.device) -> "PolicyBatch":
        return self.map(lambda tensor: tensor.to(device))

    def relabeled(self, generator: torch.Generator) -> "PolicyBatch":
        """The same decisions under random tile symmetries (see `mjers.augment`)."""
        relabeling = random_relabeling(len(self.actions), generator, self.actions.device)
        return PolicyBatch(
            relabel_tiles(self.tiles, relabeling),
            self.context,
            relabel_mask(self.action_mask, relabeling),
            relabel_actions(self.actions, relabeling),
            self.returns,
            None if self.targets is None else relabel_mask(self.targets, relabeling),
        )

    def policy_loss(self, logits: Tensor, reduction: Literal["mean", "sum"]) -> Tensor:
        if self.targets is None:
            return F.cross_entropy(logits, self.actions, reduction=reduction)
        # Masked log-probabilities are -inf and their targets 0; zero them so 0 * -inf never appears.
        log_probs = logits.log_softmax(dim=1).masked_fill(~self.action_mask, 0.0)
        losses = -(self.targets * log_probs).sum(dim=1)
        return losses.mean() if reduction == "mean" else losses.sum()


@dataclass(frozen=True)
class Metrics:
    loss: float
    correct: int
    decisions: int
    choice_correct: int
    choices: int
    uniform_loss: float
    # Mean squared error of scaled settlement predictions; None without a value head.
    value_loss: float | None


@dataclass(frozen=True)
class EpochMetrics:
    epoch: int
    training: Metrics
    validation: Metrics


@dataclass(frozen=True)
class TrainingReport:
    config: TrainingConfig
    history: tuple[EpochMetrics, ...]
    best_epoch: int


@dataclass(frozen=True)
class TrainingResult:
    policy: TrainedPolicy
    report: TrainingReport


def measure(network: PolicyNetwork, data: PolicyBatch, batch_size: int, device: torch.device) -> Metrics:
    """Metrics over host-resident `data`, moving one batch at a time to `device`."""
    network.eval()
    loss = 0.0
    uniform_loss = 0.0
    correct = 0
    choice_correct = 0
    choices = 0
    value_loss = 0.0
    has_value = isinstance(network, ResidualPolicyNet)
    n = len(data.actions)
    with torch.inference_mode():
        for start in range(0, n, batch_size):
            chunk = data.rows(start, start + batch_size).to(device)
            mask = chunk.action_mask
            labels = chunk.actions
            logits, value = network.heads(chunk.tiles, chunk.context, mask)
            if value is not None:
                value_loss += float(F.mse_loss(value, chunk.returns / VALUE_SCALE, reduction="sum").item())
            loss += float(chunk.policy_loss(logits, "sum").item())
            matches = logits.argmax(dim=1) == labels
            choice = mask.sum(dim=1) > 1
            correct += int(matches.sum().item())
            choice_correct += int((matches & choice).sum().item())
            choices += int(choice.sum().item())
            uniform_loss += float(mask.sum(dim=1).float().log().sum().item())
    return Metrics(loss / n, correct, n, choice_correct, choices, uniform_loss / n, value_loss / n if has_value else None)


def probabilities(teacher: TrainedPolicy, data: PolicyBatch, batch_size: int, device: torch.device) -> Tensor:
    """The teacher's move probabilities for every row of host-resident `data`, on the host."""
    teacher.network.eval()
    rows: list[Tensor] = []
    with torch.inference_mode():
        for start in range(0, len(data.actions), batch_size):
            chunk = data.rows(start, start + batch_size).to(device)
            rows.append(teacher.network.forward(chunk.tiles, chunk.context, chunk.action_mask).softmax(dim=1).cpu())
    return torch.cat(rows)


def train(
    training: DecisionDataset,
    validation: DecisionDataset,
    config: TrainingConfig,
    report_epoch: Callable[[EpochMetrics], None],
    report_best: Callable[["TrainingResult"], None],
) -> TrainingResult:
    """`report_best` receives the policy each time validation loss improves, so
    interrupted runs can keep their best checkpoint so far."""
    if training.metadata != validation.metadata:
        raise ValueError("training and validation must use the same encoding, rules, and teacher")
    if set(hand_seeds(training.seeds)) & set(hand_seeds(validation.seeds)):
        raise ValueError("training and validation hand seeds overlap")
    torch.set_num_threads(config.threads)
    manual_seed(config.seed)
    device = torch.device(config.device)
    network = build_network(config.architecture, config.hidden_width, RESIDUAL_BLOCKS, TILE_PLANES).to(device)
    # Data stays in host memory; only batches visit the device.
    host = torch.device("cpu")
    training_batch = PolicyBatch.from_dataset(training, host)
    validation_batch = PolicyBatch.from_dataset(validation, host)
    metadata = PolicyMetadata(
        config.architecture, config.hidden_width, training.metadata, hand_seeds(training.seeds), hand_seeds(validation.seeds)
    )
    if config.soft_teacher is not None:
        teacher = load_policy(Path(config.soft_teacher), device)
        if teacher.metadata.dataset.rules_json != training.metadata.rules_json:
            raise ValueError("the soft teacher was trained under different rules")
        training_batch = replace(training_batch, targets=probabilities(teacher, training_batch, config.batch_size, device))
        validation_batch = replace(validation_batch, targets=probabilities(teacher, validation_batch, config.batch_size, device))
        # The student learns from what the teacher trained on, so it inherits those seeds.
        inherited = set(teacher.metadata.training_seeds)
        if inherited & set(metadata.validation_seeds):
            raise ValueError("validation hands overlap the soft teacher's training")
        metadata = replace(
            metadata,
            training_seeds=tuple(sorted(inherited | set(metadata.training_seeds))),
            reinforcement=teacher.metadata.reinforcement,
        )
    optimizer = Adam(network.parameters(), lr=config.learning_rate)
    generator = torch.Generator(device="cpu").manual_seed(config.seed)
    # A separate stream keeps the shuffle order independent of augmentation.
    symmetries = torch.Generator(device="cpu").manual_seed(config.seed + 1)

    initial = EpochMetrics(
        0, measure(network, training_batch, config.batch_size, device), measure(network, validation_batch, config.batch_size, device)
    )
    history = [initial]
    report_epoch(initial)
    best_epoch = 0
    best_loss = initial.validation.loss
    best_weights = {name: tensor.detach().clone() for name, tensor in network.state_dict().items()}
    for epoch in range(1, config.epochs + 1):
        network.train(True)
        order = torch.randperm(len(training.actions), generator=generator, device="cpu")
        for start in range(0, len(order), config.batch_size):
            indices = order[start : start + config.batch_size]
            batch = training_batch.select(indices).to(device)
            if config.augment == "symmetry":
                batch = batch.relabeled(symmetries)
            learn_batch(network, optimizer, batch, config.value_weight)
        metrics = EpochMetrics(
            epoch,
            measure(network, training_batch, config.batch_size, device),
            measure(network, validation_batch, config.batch_size, device),
        )
        history.append(metrics)
        report_epoch(metrics)
        if metrics.validation.loss < best_loss:
            best_epoch = epoch
            best_loss = metrics.validation.loss
            best_weights = {name: tensor.detach().clone() for name, tensor in network.state_dict().items()}
            report_best(TrainingResult(TrainedPolicy(network, metadata, device), TrainingReport(config, tuple(history), best_epoch)))

    network.load_state_dict(best_weights, strict=True)
    network.eval()
    return TrainingResult(TrainedPolicy(network, metadata, device), TrainingReport(config, tuple(history), best_epoch))


def learn_batch(network: PolicyNetwork, optimizer: Optimizer, batch: PolicyBatch, value_weight: float) -> None:
    optimizer.zero_grad(set_to_none=True)
    logits, value = network.heads(batch.tiles, batch.context, batch.action_mask)
    loss = batch.policy_loss(logits, "mean")
    if value is not None and value_weight > 0:
        loss = loss + value_weight * F.smooth_l1_loss(value, batch.returns / VALUE_SCALE)
    if not torch.isfinite(loss):
        raise ValueError("training produced a non-finite loss")
    backward(loss)
    optimizer.step()


class TrainArguments(argparse.Namespace):
    training: list[Path]
    validation: Path
    output: Path
    architecture: Architecture
    hidden_width: int
    epochs: int
    batch_size: int
    learning_rate: float
    seed: int
    device: DeviceName
    threads: int
    augment: Augmentation
    value_weight: float
    soft_teacher: str


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--training", type=Path, nargs="+", required=True)
    parser.add_argument("--validation", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--architecture", choices=("mlp-relu-v1", "suit-cnn-relu-v1", "suit-resnet-v2"), required=True)
    parser.add_argument("--hidden-width", type=int, required=True)
    parser.add_argument("--epochs", type=int, required=True)
    parser.add_argument("--batch-size", type=int, required=True)
    parser.add_argument("--learning-rate", type=float, required=True)
    parser.add_argument("--seed", type=int, required=True)
    parser.add_argument("--device", choices=("cpu", "mps", "cuda"), required=True)
    parser.add_argument("--threads", type=int, required=True)
    parser.add_argument(
        "--augment", choices=("none", "symmetry"), required=True, help="symmetry: random suit, rank-direction and honor relabelings"
    )
    parser.add_argument("--value-weight", type=float, required=True, help="settlement prediction loss weight; suit-resnet-v2 only")
    parser.add_argument(
        "--soft-teacher", required=True, help="policy checkpoint whose move probabilities to learn instead of the labels, or none"
    )
    args = parser.parse_args(namespace=TrainArguments())
    config = TrainingConfig(
        args.architecture,
        args.hidden_width,
        args.epochs,
        args.batch_size,
        args.learning_rate,
        args.seed,
        args.device,
        args.threads,
        args.augment,
        args.value_weight,
        None if args.soft_teacher == "none" else args.soft_teacher,
    )

    def save(result: TrainingResult) -> None:
        save_policy(args.output, result.policy)
        args.output.with_suffix(".training.json").write_bytes(msgspec.json.encode(result.report))

    result = train(load_datasets(args.training), load_dataset(args.validation), config, log_epoch, save)
    save(result)
    print(f"saved epoch {result.report.best_epoch} to {args.output}")


def log_epoch(metrics: EpochMetrics) -> None:
    print(msgspec.json.encode(metrics).decode(), flush=True)


if __name__ == "__main__":
    main()
