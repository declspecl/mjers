"""Collect one imitation-data shard from complete teacher-played hands."""

import argparse
from pathlib import Path

import numpy as np
from numpy import stack

from mjers._native import ENCODING_VERSION, Env, Observation
from mjers.data import DatasetMetadata, DecisionDataset, save_dataset


def collect(env: Env, seeds: range) -> DecisionDataset:
    if len(seeds) == 0:
        raise ValueError("collection requires hand seeds")
    observations: list[Observation] = []
    actions: list[int] = []
    hand_seeds: list[int] = []
    returns: list[int] = []
    for seed in seeds:
        observation = env.reset(seed, dealer=seed % 4)
        first = len(observations)
        while observation is not None:
            observations.append(observation)
            actions.append(env.teacher_action())
            hand_seeds.append(seed)
            transition = env.step(actions[-1])
            observation = transition.observation
            if transition.terminated:
                returns.extend(transition.rewards[row.seat] for row in observations[first:])
    return DecisionDataset(
        metadata=DatasetMetadata(ENCODING_VERSION, env.rules_json, env.teacher_json),
        tiles=stack([row.tiles for row in observations]),
        context=stack([row.context for row in observations]),
        action_mask=stack([row.action_mask for row in observations]),
        actions=np.asarray(actions, dtype=np.int64),
        returns=np.asarray(returns, dtype=np.float32),
        seats=np.asarray([row.seat for row in observations], dtype=np.uint8),
        seeds=np.asarray(hand_seeds, dtype=np.uint64),
    )


class CollectArguments(argparse.Namespace):
    output: Path
    teacher: str
    rules: str
    start_seed: int
    hands: int


def read_spec(value: str) -> str:
    path = Path(value)
    return path.read_text() if path.is_file() else value


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--teacher", required=True, help="preset or BotSpec JSON file")
    parser.add_argument("--rules", required=True, help="preset or Ruleset JSON file")
    parser.add_argument("--start-seed", type=int, required=True)
    parser.add_argument("--hands", type=int, required=True)
    args = parser.parse_args(namespace=CollectArguments())
    if args.hands <= 0:
        parser.error("--hands must be positive")
    env = Env(teacher=read_spec(args.teacher), rules=read_spec(args.rules))
    dataset = collect(env, range(args.start_seed, args.start_seed + args.hands))
    save_dataset(args.output, dataset)
    print(f"wrote {len(dataset.actions)} decisions from {args.hands} hands to {args.output}")


if __name__ == "__main__":
    main()
