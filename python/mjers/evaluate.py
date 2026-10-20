"""Duplicate-seat comparison against a teacher field on untouched hand seeds."""

import argparse
import math
from dataclasses import dataclass
from pathlib import Path
from statistics import mean, stdev

import msgspec
import torch

from mjers._native import Env
from mjers.collect import read_spec
from mjers.model import DeviceName, TrainedPolicy, load_policy


@dataclass(frozen=True)
class WallResult:
    seed: int
    candidate_units_by_seat: tuple[int, ...]
    baseline_units_by_seat: tuple[int, ...]

    def __post_init__(self) -> None:
        if len(self.candidate_units_by_seat) != 4 or len(self.baseline_units_by_seat) != 4:
            raise ValueError("wall results require returns for all four seats")

    def candidate_mean(self) -> float:
        return mean(self.candidate_units_by_seat)

    def paired_delta(self) -> float:
        return mean(
            candidate - baseline for candidate, baseline in zip(self.candidate_units_by_seat, self.baseline_units_by_seat, strict=True)
        )


@dataclass(frozen=True)
class Comparison:
    rules_json: str
    baseline_json: str
    seeds: tuple[int, ...]
    candidate_units_per_hand: float
    delta_units_per_hand: float
    paired_standard_error: float
    walls: tuple[WallResult, ...]


@dataclass(frozen=True)
class PairedDifference:
    seeds: tuple[int, ...]
    second_minus_first_units_per_hand: float
    paired_standard_error: float


def paired_difference(first: Comparison, second: Comparison) -> PairedDifference:
    if first.rules_json != second.rules_json or first.baseline_json != second.baseline_json:
        raise ValueError("paired reports require identical rules and opponents")
    if first.seeds != second.seeds or len(first.seeds) < 2:
        raise ValueError("paired reports require the same independent wall seeds")
    if tuple(wall.seed for wall in first.walls) != first.seeds or tuple(wall.seed for wall in second.walls) != second.seeds:
        raise ValueError("paired report wall results do not match its seeds")
    if any(a.baseline_units_by_seat != b.baseline_units_by_seat for a, b in zip(first.walls, second.walls, strict=True)):
        raise ValueError("paired reports have inconsistent baseline results")
    deltas = [b.candidate_mean() - a.candidate_mean() for a, b in zip(first.walls, second.walls, strict=True)]
    return PairedDifference(first.seeds, mean(deltas), stdev(deltas) / math.sqrt(len(deltas)))


def play_candidate(env: Env, seed: int, seat: int, policy: TrainedPolicy) -> list[int]:
    observation = env.reset(seed, seed % 4)
    while observation is not None:
        action = policy.act(observation) if observation.seat == seat else env.teacher_action()
        transition = env.step(action)
        if transition.terminated:
            return transition.rewards
        observation = transition.observation
    raise RuntimeError("hand ended without a settlement")


def play_teacher(env: Env, seed: int) -> list[int]:
    observation = env.reset(seed, seed % 4)
    while observation is not None:
        transition = env.step(env.teacher_action())
        if transition.terminated:
            return transition.rewards
        observation = transition.observation
    raise RuntimeError("hand ended without a settlement")


def compare(policy: TrainedPolicy, env: Env, seeds: range) -> Comparison:
    if len(seeds) < 2:
        raise ValueError("comparison needs at least two independent hand seeds")
    if policy.metadata.dataset.rules_json != env.rules_json:
        raise ValueError("evaluation rules differ from the checkpoint's training rules")
    seen = set(policy.metadata.training_seeds) | set(policy.metadata.validation_seeds)
    if seen.intersection(seeds):
        raise ValueError("evaluation hand seeds overlap training or validation")
    walls: list[WallResult] = []
    for seed in seeds:
        baseline = tuple(play_teacher(env, seed))
        candidate = tuple(play_candidate(env, seed, seat, policy)[seat] for seat in range(4))
        walls.append(WallResult(seed, candidate, baseline))
    # Four seat rotations of one wall are correlated. Estimate uncertainty
    # across independent walls, retaining the within-wall pairing.
    deltas = [wall.paired_delta() for wall in walls]
    error = stdev(deltas) / math.sqrt(len(deltas))
    return Comparison(
        env.rules_json, env.teacher_json, tuple(seeds), mean(wall.candidate_mean() for wall in walls), mean(deltas), error, tuple(walls)
    )


class EvaluateArguments(argparse.Namespace):
    checkpoint: Path
    baseline: str
    rules: str
    start_seed: int
    hands: int
    device: DeviceName
    threads: int
    output: Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--checkpoint", type=Path, required=True)
    parser.add_argument("--baseline", required=True, help="preset or BotSpec JSON file")
    parser.add_argument("--rules", required=True, help="preset or Ruleset JSON file")
    parser.add_argument("--start-seed", type=int, required=True)
    parser.add_argument("--hands", type=int, required=True, help="independent walls; candidate plays every seat")
    parser.add_argument("--device", choices=("cpu", "mps", "cuda"), required=True)
    parser.add_argument("--threads", type=int, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(namespace=EvaluateArguments())
    if args.threads <= 0:
        parser.error("--threads must be positive")
    torch.set_num_threads(args.threads)
    policy = load_policy(args.checkpoint, torch.device(args.device))
    env = Env(teacher=read_spec(args.baseline), rules=read_spec(args.rules))
    result = compare(policy, env, range(args.start_seed, args.start_seed + args.hands))
    encoded = msgspec.json.encode(result)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(encoded)
    print(encoded.decode())


if __name__ == "__main__":
    main()
