"""Improve a policy with a value head by playing one seat against a fixed field (PPO).

Each hand, the student plays one randomly chosen seat; the other three follow
the environment's teacher, which here is the opponent field. The only reward is
the student's settlement, scaled by `VALUE_SCALE`; advantages use the value
head with generalized advantage estimation over the student's own decisions.
Available wins are always taken and single-action decisions are not learned.
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

from mjers._native import Env, Observation
from mjers.collect import read_spec
from mjers.evaluate import range_overlaps
from mjers.model import VALUE_SCALE, DeviceName, Reinforcement, ResidualPolicyNet, TrainedPolicy, load_policy, save_policy

WIN_ACTIONS = (107, 108)


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
    gae_lambda: float
    seed: int
    start_seed: int
    device: DeviceName
    threads: int

    def __post_init__(self) -> None:
        counts = (self.iterations, self.hands_per_iteration, self.environments, self.epochs, self.minibatch_size, self.threads)
        if min(counts) <= 0:
            raise ValueError("iteration, hand, environment, epoch, minibatch, and thread counts must be positive")
        if not math.isfinite(self.learning_rate) or self.learning_rate <= 0:
            raise ValueError("learning rate must be positive and finite")
        if not 0 < self.clip < 1 or not 0 <= self.gae_lambda <= 1:
            raise ValueError("clip must be in (0, 1) and GAE lambda in [0, 1]")
        if min(self.value_weight, self.entropy_weight) < 0:
            raise ValueError("loss weights must be nonnegative")

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


class Hand:
    """One environment's current hand and the student's decisions in it."""

    def __init__(self, env: Env, seed: int, seat: int) -> None:
        self.env = env
        self.seed = seed
        self.seat = seat
        self.observation: Observation | None = env.reset(seed, seed % 4)
        self.observations: list[Observation] = []
        self.actions: list[int] = []
        self.log_probs: list[float] = []
        self.values: list[float] = []
        self.units: int | None = None

    def advance(self) -> None:
        """Play the field and forced student actions until the student must choose."""
        while self.observation is not None:
            observation = self.observation
            if observation.seat == self.seat:
                legal = np.flatnonzero(observation.action_mask)
                wins = [action for action in WIN_ACTIONS if observation.action_mask[action]]
                if not wins and len(legal) > 1:
                    return
                self.step(wins[0] if wins else int(legal[0]))
            else:
                self.step(self.env.teacher_action())

    def choose(self, action: int, log_prob: float, value: float) -> None:
        assert self.observation is not None
        self.observations.append(self.observation)
        self.actions.append(action)
        self.log_probs.append(log_prob)
        self.values.append(value)
        self.step(action)

    def step(self, action: int) -> None:
        transition = self.env.step(action)
        self.observation = transition.observation
        if transition.terminated:
            self.units = transition.rewards[self.seat]


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
    envs: list[Env],
    seeds: Iterator[int],
    hands: int,
    rng: np.random.Generator,
    generator: torch.Generator,
    device: torch.device,
    gae_lambda: float,
) -> tuple[Experience, list[int]]:
    network.eval()
    started = 0
    finished: list[Hand] = []

    def start(env: Env) -> Hand:
        nonlocal started
        started += 1
        hand = Hand(env, next(seeds), int(rng.integers(4)))
        hand.advance()
        return hand

    active = [start(env) for env in envs[:hands]]
    while active:
        waiting = [hand for hand in active if hand.observation is not None]
        if waiting:
            observations = [hand.observation for hand in waiting if hand.observation is not None]
            tiles = torch.from_numpy(np.stack([o.tiles for o in observations])).to(device)
            context = torch.from_numpy(np.stack([o.context for o in observations])).to(device)
            mask = torch.from_numpy(np.stack([o.action_mask for o in observations])).to(device)
            with torch.inference_mode():
                logits, values = network.heads(tiles, context, mask)
                log_probs = logits.log_softmax(dim=1)
                actions = torch.multinomial(log_probs.exp().cpu(), 1, generator=generator).squeeze(1)
                chosen = log_probs.cpu().gather(1, actions.unsqueeze(1)).squeeze(1)
            for hand, action, log_prob, value in zip(waiting, actions.tolist(), chosen.tolist(), values.cpu().tolist(), strict=True):
                hand.choose(action, log_prob, value)
                hand.advance()
        still: list[Hand] = []
        for hand in active:
            if hand.observation is None:
                finished.append(hand)
                if started < hands:
                    still.append(start(hand.env))
            else:
                still.append(hand)
        active = still
    return experience(finished, device, gae_lambda), [hand.units if hand.units is not None else 0 for hand in finished]


def experience(hands: list[Hand], device: torch.device, gae_lambda: float) -> Experience:
    rows = [hand for hand in hands if hand.observations]
    advantage_rows = [advantages(np.asarray(hand.values, dtype=np.float32), (hand.units or 0) / VALUE_SCALE, gae_lambda) for hand in rows]
    values = np.concatenate([np.asarray(hand.values, dtype=np.float32) for hand in rows])
    advantage = np.concatenate(advantage_rows)
    observations = [observation for hand in rows for observation in hand.observations]
    return Experience(
        torch.from_numpy(np.stack([o.tiles for o in observations])).to(device),
        torch.from_numpy(np.stack([o.context for o in observations])).to(device),
        torch.from_numpy(np.stack([o.action_mask for o in observations])).to(device),
        torch.tensor([action for hand in rows for action in hand.actions], dtype=torch.int64, device=device),
        torch.tensor([p for hand in rows for p in hand.log_probs], dtype=torch.float32, device=device),
        torch.from_numpy(advantage).to(device),
        torch.from_numpy(advantage + values).to(device),
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
            entropy = -(legal_log_probs.exp() * batch.action_mask * legal_log_probs).sum(dim=1).mean()
            loss = policy_loss + config.value_weight * value_loss - config.entropy_weight * entropy
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
    report_iteration: Callable[[IterationReport], None],
) -> tuple[TrainedPolicy, tuple[IterationReport, ...]]:
    network = policy.network
    if not isinstance(network, ResidualPolicyNet):
        raise ValueError("reinforcement needs a policy with a value head (suit-resnet-v2)")
    stage = Reinforcement(field_json, config.start_seed, config.hands())
    seen = set(policy.metadata.training_seeds) | set(policy.metadata.validation_seeds)
    if seen.intersection(stage.seeds()) or any(range_overlaps(earlier.seeds(), stage.seeds()) for earlier in policy.metadata.reinforcement):
        raise ValueError("reinforcement hand seeds overlap earlier training")
    torch.set_num_threads(config.threads)
    torch.manual_seed(config.seed)
    device = torch.device(config.device)
    network = network.to(device)
    envs = [Env(teacher=field_json, rules=rules_json) for _ in range(config.environments)]
    if envs[0].rules_json != policy.metadata.dataset.rules_json:
        raise ValueError("reinforcement rules differ from the policy's training rules")
    seeds = iter(stage.seeds())
    rng = np.random.default_rng(config.seed)
    sampling = torch.Generator().manual_seed(config.seed)
    shuffling = torch.Generator().manual_seed(config.seed + 1)
    optimizer = Adam(network.parameters(), lr=config.learning_rate)
    reports: list[IterationReport] = []
    for iteration in range(1, config.iterations + 1):
        began = time.perf_counter()
        data, units = play(network, envs, seeds, config.hands_per_iteration, rng, sampling, device, config.gae_lambda)
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
        report_iteration(report)
    network.eval()
    metadata = replace(policy.metadata, reinforcement=(*policy.metadata.reinforcement, stage))
    return TrainedPolicy(network, metadata, device), tuple(reports)


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
    gae_lambda: float
    seed: int
    start_seed: int
    device: DeviceName
    threads: int


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
    parser.add_argument("--gae-lambda", type=float, required=True)
    parser.add_argument("--seed", type=int, required=True)
    parser.add_argument("--start-seed", type=int, required=True, help="first hand seed; hands use a disjoint consecutive block")
    parser.add_argument("--device", choices=("cpu", "mps", "cuda"), required=True)
    parser.add_argument("--threads", type=int, required=True)
    args = parser.parse_args(namespace=ReinforceArguments())
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
        args.gae_lambda,
        args.seed,
        args.start_seed,
        args.device,
        args.threads,
    )
    initial = load_policy(args.checkpoint, torch.device(args.device))
    field_json = Env(teacher=read_spec(args.field), rules=read_spec(args.rules)).teacher_json
    policy, iterations = reinforce(initial, field_json, read_spec(args.rules), config, log_iteration)
    save_policy(args.output, policy)
    report = ReinforcementReport(config, field_json, str(args.checkpoint), iterations)
    args.output.with_suffix(".reinforcement.json").write_bytes(msgspec.json.encode(report))
    print(f"saved {args.output}")


def log_iteration(report: IterationReport) -> None:
    print(msgspec.json.encode(report).decode(), flush=True)


if __name__ == "__main__":
    main()
