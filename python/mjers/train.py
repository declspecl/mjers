"""Train the masked MLP by imitation on disjoint hand-seed datasets."""

import argparse
import math
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path

import msgspec
import torch
from torch import Tensor, from_numpy, manual_seed
from torch.autograd import backward
from torch.nn import functional as F
from torch.optim import Adam, Optimizer

from mjers.data import DecisionDataset, load_dataset
from mjers.model import DeviceName, PolicyMetadata, PolicyNet, TrainedPolicy, hand_seeds, save_policy


@dataclass(frozen=True)
class TrainingConfig:
    hidden_width: int
    epochs: int
    batch_size: int
    learning_rate: float
    seed: int
    device: DeviceName
    threads: int

    def __post_init__(self) -> None:
        if min(self.hidden_width, self.epochs, self.batch_size, self.threads) <= 0:
            raise ValueError("width, epochs, batch size, and thread count must be positive")
        if not math.isfinite(self.learning_rate) or self.learning_rate <= 0:
            raise ValueError("learning rate must be positive and finite")


@dataclass(frozen=True)
class PolicyBatch:
    tiles: Tensor
    context: Tensor
    action_mask: Tensor
    actions: Tensor

    @classmethod
    def from_dataset(cls, dataset: DecisionDataset, device: torch.device) -> "PolicyBatch":
        return cls(
            from_numpy(dataset.tiles).to(device),
            from_numpy(dataset.context).to(device),
            from_numpy(dataset.action_mask).to(device),
            from_numpy(dataset.actions).to(device),
        )

    def select(self, indices: Tensor) -> "PolicyBatch":
        return PolicyBatch(self.tiles[indices], self.context[indices], self.action_mask[indices], self.actions[indices])


@dataclass(frozen=True)
class Metrics:
    loss: float
    correct: int
    decisions: int
    choice_correct: int
    choices: int
    uniform_loss: float


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


def measure(network: PolicyNet, data: PolicyBatch, batch_size: int) -> Metrics:
    network.eval()
    loss = 0.0
    uniform_loss = 0.0
    correct = 0
    choice_correct = 0
    choices = 0
    n = len(data.actions)
    with torch.inference_mode():
        for start in range(0, n, batch_size):
            end = start + batch_size
            mask = data.action_mask[start:end]
            labels = data.actions[start:end]
            logits = network.forward(data.tiles[start:end], data.context[start:end], mask)
            loss += float(F.cross_entropy(logits, labels, reduction="sum").item())
            matches = logits.argmax(dim=1) == labels
            choice = mask.sum(dim=1) > 1
            correct += int(matches.sum().item())
            choice_correct += int((matches & choice).sum().item())
            choices += int(choice.sum().item())
            uniform_loss += float(mask.sum(dim=1).float().log().sum().item())
    return Metrics(loss / n, correct, n, choice_correct, choices, uniform_loss / n)


def train(
    training: DecisionDataset,
    validation: DecisionDataset,
    config: TrainingConfig,
    report_epoch: Callable[[EpochMetrics], None],
) -> TrainingResult:
    if training.metadata != validation.metadata:
        raise ValueError("training and validation must use the same encoding, rules, and teacher")
    if set(hand_seeds(training.seeds)) & set(hand_seeds(validation.seeds)):
        raise ValueError("training and validation hand seeds overlap")
    torch.set_num_threads(config.threads)
    manual_seed(config.seed)
    device = torch.device(config.device)
    network = PolicyNet(config.hidden_width).to(device)
    training_batch = PolicyBatch.from_dataset(training, device)
    validation_batch = PolicyBatch.from_dataset(validation, device)
    optimizer = Adam(network.parameters(), lr=config.learning_rate)
    generator = torch.Generator(device="cpu").manual_seed(config.seed)

    initial = EpochMetrics(0, measure(network, training_batch, config.batch_size), measure(network, validation_batch, config.batch_size))
    history = [initial]
    report_epoch(initial)
    best_epoch = 0
    best_loss = initial.validation.loss
    best_weights = {name: tensor.detach().clone() for name, tensor in network.state_dict().items()}
    for epoch in range(1, config.epochs + 1):
        network.train(True)
        order = torch.randperm(len(training.actions), generator=generator, device="cpu").to(device)
        for start in range(0, len(order), config.batch_size):
            indices = order[start : start + config.batch_size]
            batch = training_batch.select(indices)
            learn_batch(network, optimizer, batch)
        metrics = EpochMetrics(
            epoch, measure(network, training_batch, config.batch_size), measure(network, validation_batch, config.batch_size)
        )
        history.append(metrics)
        report_epoch(metrics)
        if metrics.validation.loss < best_loss:
            best_epoch = epoch
            best_loss = metrics.validation.loss
            best_weights = {name: tensor.detach().clone() for name, tensor in network.state_dict().items()}

    network.load_state_dict(best_weights, strict=True)
    network.eval()
    metadata = PolicyMetadata(
        "mlp-relu-v1", config.hidden_width, training.metadata, hand_seeds(training.seeds), hand_seeds(validation.seeds)
    )
    return TrainingResult(TrainedPolicy(network, metadata, device), TrainingReport(config, tuple(history), best_epoch))


def learn_batch(network: PolicyNet, optimizer: Optimizer, batch: PolicyBatch) -> None:
    optimizer.zero_grad(set_to_none=True)
    logits = network.forward(batch.tiles, batch.context, batch.action_mask)
    loss = F.cross_entropy(logits, batch.actions)
    if not torch.isfinite(loss):
        raise ValueError("training produced a non-finite loss")
    backward(loss)
    optimizer.step()


class TrainArguments(argparse.Namespace):
    training: Path
    validation: Path
    output: Path
    hidden_width: int
    epochs: int
    batch_size: int
    learning_rate: float
    seed: int
    device: DeviceName
    threads: int


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--training", type=Path, required=True)
    parser.add_argument("--validation", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--hidden-width", type=int, required=True)
    parser.add_argument("--epochs", type=int, required=True)
    parser.add_argument("--batch-size", type=int, required=True)
    parser.add_argument("--learning-rate", type=float, required=True)
    parser.add_argument("--seed", type=int, required=True)
    parser.add_argument("--device", choices=("cpu", "mps", "cuda"), required=True)
    parser.add_argument("--threads", type=int, required=True)
    args = parser.parse_args(namespace=TrainArguments())
    config = TrainingConfig(args.hidden_width, args.epochs, args.batch_size, args.learning_rate, args.seed, args.device, args.threads)
    result = train(load_dataset(args.training), load_dataset(args.validation), config, log_epoch)
    save_policy(args.output, result.policy)
    args.output.with_suffix(".training.json").write_bytes(msgspec.json.encode(result.report))
    print(f"saved epoch {result.report.best_epoch} to {args.output}")


def log_epoch(metrics: EpochMetrics) -> None:
    print(msgspec.json.encode(metrics).decode(), flush=True)


if __name__ == "__main__":
    main()
