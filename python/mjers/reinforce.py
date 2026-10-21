"""Improve a policy with a value head by playing one seat against a field (PPO).

Each hand, the student plays one randomly chosen seat in a Rust `Arena`; the
other three seats are the opponent field: a fixed strategy, or with
`--snapshot-every N` a frozen copy of the student refreshed every N iterations
(self-play). The only reward is the student's settlement, scaled by
`VALUE_SCALE`; advantages use the value head with generalized advantage
estimation over the student's own decisions. Available wins are always taken
and single-action decisions are not learned.
"""

import argparse
import math
import time
from collections.abc import Callable, Iterator
from dataclasses import dataclass, replace
from pathlib import Path

import msgspec
import numpy as np
import torch
from numpy.typing import NDArray
from torch import Tensor
from torch.autograd import backward
from torch.nn import functional as F
from torch.optim import Adam

from mjers._native import PASS_ACTION, Arena, Env
from mjers.collect import read_spec
from mjers.evaluate import range_overlaps
from mjers.model import VALUE_SCALE, DeviceName, Reinforcement, ResidualPolicyNet, TrainedPolicy, load_policy, native_network, save_policy


@dataclass(frozen=True)
class ReinforcementConfig:
    iterations: int
    hands_per_iteration: int
    environments: int
    epochs: int
    minibatch_size: int
    learning_rate: float
    clip: float
    value_weight: float
    entropy_weight: float
    # Added to entropy_weight on claim responses, which imitation leaves nearly deterministic.
    claim_entropy_weight: float
    gae_lambda: float
    seed: int
    start_seed: int
    device: DeviceName
    threads: int
    # 0 plays the fixed field; N > 0 replaces it with a frozen copy of the
    # student (self-play), refreshed every N iterations.
    snapshot_every: int

    def __post_init__(self) -> None:
        counts = (self.iterations, self.hands_per_iteration, self.environments, self.epochs, self.minibatch_size, self.threads)
        if min(counts) <= 0:
            raise ValueError("iteration, hand, environment, epoch, minibatch, and thread counts must be positive")
        if not math.isfinite(self.learning_rate) or self.learning_rate <= 0:
            raise ValueError("learning rate must be positive and finite")
        if not 0 < self.clip < 1 or not 0 <= self.gae_lambda <= 1:
            raise ValueError("clip must be in (0, 1) and GAE lambda in [0, 1]")
        if min(self.value_weight, self.entropy_weight, self.claim_entropy_weight) < 0:
            raise ValueError("loss weights must be nonnegative")
        if self.snapshot_every < 0:
            raise ValueError("snapshot interval must be nonnegative")

    def hands(self) -> int:
        return self.iterations * self.hands_per_iteration


@dataclass(frozen=True)
class Experience:
    """Student decisions with their sampling log-probabilities and GAE targets."""

    tiles: Tensor
    context: Tensor
    action_mask: Tensor
    actions: Tensor
    log_probs: Tensor
    advantages: Tensor
    returns: Tensor

    def select(self, indices: Tensor) -> "Experience":
        return Experience(
            self.tiles[indices],
            self.context[indices],
            self.action_mask[indices],
            self.actions[indices],
            self.log_probs[indices],
            self.advantages[indices],
            self.returns[indices],
        )


@dataclass(frozen=True)
class IterationReport:
    iteration: int
    hands: int
    decisions: int
    mean_units: float
    units_standard_error: float
    entropy: float
    approximate_kl: float
    clip_fraction: float
    value_loss: float
    hands_per_second: float


@dataclass(frozen=True)
class ReinforcementReport:
    config: ReinforcementConfig
    field_json: str
    initial_checkpoint: str
    iterations: tuple[IterationReport, ...]


class Trajectory:
    """The student's real choices in one hand, in order."""

    def __init__(self) -> None:
        self.tiles: list[NDArray[np.float32]] = []
        self.context: list[NDArray[np.float32]] = []
        self.masks: list[NDArray[np.bool_]] = []
        self.actions: list[int] = []
        self.log_probs: list[float] = []
        self.values: list[float] = []


@dataclass(frozen=True)
class Played:
    trajectory: Trajectory
    units: int


def advantages(values: NDArray[np.float32], reward: float, gae_lambda: float) -> NDArray[np.float32]:
    """GAE with no discount and a single terminal reward."""
    following = np.append(values[1:], reward).astype(np.float32)
    deltas = following - values
    result = np.zeros_like(values)
    running = 0.0
    for step in range(len(values) - 1, -1, -1):
        running = float(deltas[step]) + gae_lambda * running
        result[step] = running
    return result


def play(
    network: ResidualPolicyNet,
    arena: Arena,
    slots: int,
    seeds: Iterator[int],
    hands: int,
    rng: np.random.Generator,
    generator: torch.Generator,
    device: torch.device,
    gae_lambda: float,
) -> tuple[Experience, list[int]]:
    """Play `hands` hands, `slots` at a time; the field and forced moves run in Rust."""
    network.eval()
    started = 0
    active: dict[int, Trajectory] = {}
    finished: list[Played] = []

    def start(slot: int) -> None:
        nonlocal started
        started += 1
        active[slot] = Trajectory()
        arena.start(slot, next(seeds), int(rng.integers(4)))

    def collect_finished() -> None:
        # Starting a hand can finish it at once, so drain until nothing new ends.
        while done := arena.take_finished():
            for slot, _seed, _seat, units in done:
                finished.append(Played(active.pop(slot), units))
                if started < hands:
                    start(slot)

    for slot in range(min(slots, hands)):
        start(slot)
    collect_finished()
    while active:
        waiting, tiles, context, mask = arena.pending()
        with torch.inference_mode():
            logits, values = network.heads(
                torch.from_numpy(tiles).to(device), torch.from_numpy(context).to(device), torch.from_numpy(mask).to(device)
            )
            log_probs = logits.log_softmax(dim=1)
            actions = torch.multinomial(log_probs.exp().cpu(), 1, generator=generator).squeeze(1)
            chosen = log_probs.cpu().gather(1, actions.unsqueeze(1)).squeeze(1)
        for row, (slot, action, log_prob, value) in enumerate(
            zip(waiting, actions.tolist(), chosen.tolist(), values.cpu().tolist(), strict=True)
        ):
            trajectory = active[slot]
            trajectory.tiles.append(tiles[row])
            trajectory.context.append(context[row])
            trajectory.masks.append(mask[row])
            trajectory.actions.append(action)
            trajectory.log_probs.append(log_prob)
            trajectory.values.append(value)
        arena.act(waiting, actions.tolist())
        collect_finished()
    return experience(finished, device, gae_lambda), [hand.units for hand in finished]


def experience(hands: list[Played], device: torch.device, gae_lambda: float) -> Experience:
    rows = [hand for hand in hands if hand.trajectory.actions]
    values = [np.asarray(hand.trajectory.values, dtype=np.float32) for hand in rows]
    advantage = np.concatenate([advantages(v, hand.units / VALUE_SCALE, gae_lambda) for v, hand in zip(values, rows, strict=True)])
    trajectories = [hand.trajectory for hand in rows]
    return Experience(
        torch.from_numpy(np.stack([t for trajectory in trajectories for t in trajectory.tiles])).to(device),
        torch.from_numpy(np.stack([c for trajectory in trajectories for c in trajectory.context])).to(device),
        torch.from_numpy(np.stack([m for trajectory in trajectories for m in trajectory.masks])).to(device),
        torch.tensor([a for trajectory in trajectories for a in trajectory.actions], dtype=torch.int64, device=device),
        torch.tensor([p for trajectory in trajectories for p in trajectory.log_probs], dtype=torch.float32, device=device),
        torch.from_numpy(advantage).to(device),
        torch.from_numpy(advantage + np.concatenate(values)).to(device),
    )


@dataclass(frozen=True)
class UpdateStatistics:
    entropy: float
    approximate_kl: float
    clip_fraction: float
    value_loss: float


def update(
    network: ResidualPolicyNet, optimizer: Adam, data: Experience, config: ReinforcementConfig, generator: torch.Generator
) -> UpdateStatistics:
    network.train(True)
    normalized = (data.advantages - data.advantages.mean()) / (data.advantages.std() + 1e-8)
    data = replace(data, advantages=normalized)
    entropies: list[float] = []
    kls: list[float] = []
    clips: list[float] = []
    value_losses: list[float] = []
    n = len(data.actions)
    for _ in range(config.epochs):
        order = torch.randperm(n, generator=generator).to(data.actions.device)
        for start in range(0, n, config.minibatch_size):
            batch = data.select(order[start : start + config.minibatch_size])
            logits, values = network.heads(batch.tiles, batch.context, batch.action_mask)
            log_probs = logits.log_softmax(dim=1)
            chosen = log_probs.gather(1, batch.actions.unsqueeze(1)).squeeze(1)
            ratio = (chosen - batch.log_probs).exp()
            clipped = ratio.clamp(1 - config.clip, 1 + config.clip)
            policy_loss = -torch.minimum(ratio * batch.advantages, clipped * batch.advantages).mean()
            value_loss = F.smooth_l1_loss(values, batch.returns)
            # Masked log-probabilities are -inf; zero them first so 0 * -inf never reaches the gradient.
            legal_log_probs = log_probs.masked_fill(~batch.action_mask, 0.0)
            entropies_by_row = -(legal_log_probs.exp() * batch.action_mask * legal_log_probs).sum(dim=1)
            entropy = entropies_by_row.mean()
            claim_entropy = (entropies_by_row * batch.action_mask[:, PASS_ACTION]).mean()
            loss = (
                policy_loss
                + config.value_weight * value_loss
                - config.entropy_weight * entropy
                - config.claim_entropy_weight * claim_entropy
            )
            if not torch.isfinite(loss):
                raise ValueError("reinforcement produced a non-finite loss")
            optimizer.zero_grad(set_to_none=True)
            backward(loss)
            torch.nn.utils.clip_grad_norm_(network.parameters(), 1.0)
            optimizer.step()
            with torch.no_grad():
                entropies.append(float(entropy))
                kls.append(float((batch.log_probs - chosen).mean()))
                clips.append(float(((ratio - 1).abs() > config.clip).float().mean()))
                value_losses.append(float(value_loss))
    return UpdateStatistics(float(np.mean(entropies)), float(np.mean(kls)), float(np.mean(clips)), float(np.mean(value_losses)))


def reinforce(
    policy: TrainedPolicy,
    field_json: str,
    rules_json: str,
    config: ReinforcementConfig,
    report_iteration: Callable[[IterationReport, TrainedPolicy], None],
) -> tuple[TrainedPolicy, tuple[IterationReport, ...]]:
    """`report_iteration` also receives the policy so far, whose metadata covers only hands played."""
    network = policy.network
    if not isinstance(network, ResidualPolicyNet):
        raise ValueError("reinforcement needs a policy with a value head (suit-resnet-v2)")
    if config.snapshot_every > 0:
        field_json = msgspec.json.encode({"kind": "snapshot", "refresh_every": config.snapshot_every}).decode()
    stage = Reinforcement(field_json, config.start_seed, config.hands())
    seen = set(policy.metadata.training_seeds) | set(policy.metadata.validation_seeds)
    if seen.intersection(stage.seeds()) or any(range_overlaps(earlier.seeds(), stage.seeds()) for earlier in policy.metadata.reinforcement):
        raise ValueError("reinforcement hand seeds overlap earlier training")
    torch.set_num_threads(config.threads)
    torch.manual_seed(config.seed)
    device = torch.device(config.device)
    network = network.to(device)
    # The Arena needs a BotSpec even when snapshots replace the field.
    arena = Arena("efficiency" if config.snapshot_every > 0 else field_json, rules=rules_json, slots=config.environments)
    if arena.rules_json != policy.metadata.dataset.rules_json:
        raise ValueError("reinforcement rules differ from the policy's training rules")
    seeds = iter(stage.seeds())
    rng = np.random.default_rng(config.seed)
    sampling = torch.Generator().manual_seed(config.seed)
    shuffling = torch.Generator().manual_seed(config.seed + 1)
    optimizer = Adam(network.parameters(), lr=config.learning_rate)
    reports: list[IterationReport] = []
    for iteration in range(1, config.iterations + 1):
        began = time.perf_counter()
        if config.snapshot_every > 0 and (iteration - 1) % config.snapshot_every == 0:
            arena.set_field_network(f"snapshot-{iteration}", native_network(network))
        data, units = play(network, arena, config.environments, seeds, config.hands_per_iteration, rng, sampling, device, config.gae_lambda)
        statistics = update(network, optimizer, data, config, shuffling)
        spread = float(np.std(units, ddof=1)) / math.sqrt(len(units)) if len(units) > 1 else 0.0
        report = IterationReport(
            iteration,
            len(units),
            len(data.actions),
            float(np.mean(units)),
            spread,
            statistics.entropy,
            statistics.approximate_kl,
            statistics.clip_fraction,
            statistics.value_loss,
            len(units) / (time.perf_counter() - began),
        )
        reports.append(report)
        report_iteration(report, played(policy, network, replace(stage, hands=iteration * config.hands_per_iteration), device))
    network.eval()
    return played(policy, network, stage, device), tuple(reports)


def played(initial: TrainedPolicy, network: ResidualPolicyNet, stage: Reinforcement, device: torch.device) -> TrainedPolicy:
    metadata = replace(initial.metadata, reinforcement=(*initial.metadata.reinforcement, stage))
    return TrainedPolicy(network, metadata, device)


class ReinforceArguments(argparse.Namespace):
    checkpoint: Path
    field: str
    rules: str
    output: Path
    iterations: int
    hands_per_iteration: int
    environments: int
    epochs: int
    minibatch_size: int
    learning_rate: float
    clip: float
    value_weight: float
    entropy_weight: float
    claim_entropy_weight: float
    gae_lambda: float
    seed: int
    start_seed: int
    device: DeviceName
    threads: int
    checkpoint_every: int
    snapshot_every: int


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--checkpoint", type=Path, required=True, help="initial suit-resnet-v2 policy")
    parser.add_argument("--field", required=True, help="opponent preset or BotSpec JSON file")
    parser.add_argument("--rules", required=True, help="preset or Ruleset JSON file")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--iterations", type=int, required=True)
    parser.add_argument("--hands-per-iteration", type=int, required=True)
    parser.add_argument("--environments", type=int, required=True, help="hands played concurrently for batched inference")
    parser.add_argument("--epochs", type=int, required=True)
    parser.add_argument("--minibatch-size", type=int, required=True)
    parser.add_argument("--learning-rate", type=float, required=True)
    parser.add_argument("--clip", type=float, required=True)
    parser.add_argument("--value-weight", type=float, required=True)
    parser.add_argument("--entropy-weight", type=float, required=True)
    parser.add_argument("--claim-entropy-weight", type=float, required=True, help="extra entropy weight on claim responses")
    parser.add_argument("--gae-lambda", type=float, required=True)
    parser.add_argument("--seed", type=int, required=True)
    parser.add_argument("--start-seed", type=int, required=True, help="first hand seed; hands use a disjoint consecutive block")
    parser.add_argument("--device", choices=("cpu", "mps", "cuda"), required=True)
    parser.add_argument("--threads", type=int, required=True)
    parser.add_argument("--checkpoint-every", type=int, required=True, help="also save the output every this many iterations")
    parser.add_argument(
        "--snapshot-every",
        type=int,
        required=True,
        help="0 plays --field; N > 0 plays a frozen copy of the student refreshed every N iterations",
    )
    args = parser.parse_args(namespace=ReinforceArguments())
    if args.checkpoint_every <= 0:
        parser.error("--checkpoint-every must be positive")
    config = ReinforcementConfig(
        args.iterations,
        args.hands_per_iteration,
        args.environments,
        args.epochs,
        args.minibatch_size,
        args.learning_rate,
        args.clip,
        args.value_weight,
        args.entropy_weight,
        args.claim_entropy_weight,
        args.gae_lambda,
        args.seed,
        args.start_seed,
        args.device,
        args.threads,
        args.snapshot_every,
    )
    initial = load_policy(args.checkpoint, torch.device(args.device))
    field_json = Env(teacher=read_spec(args.field), rules=read_spec(args.rules)).teacher_json
    history: list[IterationReport] = []

    def save(policy: TrainedPolicy) -> None:
        save_policy(args.output, policy)
        report = ReinforcementReport(config, field_json, str(args.checkpoint), tuple(history))
        args.output.with_suffix(".reinforcement.json").write_bytes(msgspec.json.encode(report))

    def log_iteration(report: IterationReport, policy: TrainedPolicy) -> None:
        history.append(report)
        print(msgspec.json.encode(report).decode(), flush=True)
        if report.iteration % args.checkpoint_every == 0:
            save(policy)

    policy, _ = reinforce(initial, field_json, read_spec(args.rules), config, log_iteration)
    save(policy)
    print(f"saved {args.output}")


if __name__ == "__main__":
    main()
