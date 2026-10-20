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
class Comparison:
    rules_json: str
    baseline_json: str
    seeds: tuple[int, ...]
    candidate_units_per_hand: float
    delta_units_per_hand: float
    paired_standard_error: float


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
    deltas_by_seed: list[float] = []
    returns_by_seed: list[float] = []
    for seed in seeds:
        baseline = play_teacher(env, seed)
        candidate = [play_candidate(env, seed, seat, policy)[seat] for seat in range(4)]
        returns_by_seed.append(mean(candidate))
        deltas_by_seed.append(mean(candidate[seat] - baseline[seat] for seat in range(4)))
    # Four seat rotations of one wall are correlated. Estimate uncertainty
    # across independent walls, retaining the within-wall pairing.
    error = stdev(deltas_by_seed) / math.sqrt(len(deltas_by_seed))
    return Comparison(env.rules_json, env.teacher_json, tuple(seeds), mean(returns_by_seed), mean(deltas_by_seed), error)


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
