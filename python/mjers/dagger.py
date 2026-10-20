"""Collect teacher labels on positions a student policy helped reach (DAgger).

Imitation from teacher-only play never shows the student the positions its
own mistakes create. Here each decision is played by the student with a fixed
probability and by the teacher otherwise; every decision is still labeled
with the teacher's action. Returns describe this mixed play.
"""

import argparse
import math
from dataclasses import dataclass
from pathlib import Path

import msgspec
import numpy as np
import torch

from mjers._native import Env, Observation
from mjers.collect import collect_with, read_spec
from mjers.data import save_dataset
from mjers.model import DeviceName, TrainedPolicy, load_policy


@dataclass(frozen=True)
class MixedBehavior:
    student: TrainedPolicy
    probability: float
    rng: np.random.Generator

    def __post_init__(self) -> None:
        if not math.isfinite(self.probability) or not 0 <= self.probability <= 1:
            raise ValueError("student probability must be in [0, 1]")

    def __call__(self, observation: Observation, teacher: int) -> int:
        return self.student.act(observation) if self.rng.random() < self.probability else teacher


@dataclass(frozen=True)
class CollectionProvenance:
    """Written beside the shard: how its positions were reached."""

    student_checkpoint: str
    student_probability: float
    seed: int
    start_seed: int
    hands: int


class DaggerArguments(argparse.Namespace):
    output: Path
    teacher: str
    rules: str
    student: Path
    student_probability: float
    seed: int
    start_seed: int
    hands: int
    device: DeviceName
    threads: int


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--teacher", required=True, help="preset or BotSpec JSON file")
    parser.add_argument("--rules", required=True, help="preset or Ruleset JSON file")
    parser.add_argument("--student", type=Path, required=True, help="policy checkpoint")
    parser.add_argument("--student-probability", type=float, required=True)
    parser.add_argument("--seed", type=int, required=True, help="seeds the student/teacher choice")
    parser.add_argument("--start-seed", type=int, required=True)
    parser.add_argument("--hands", type=int, required=True)
    parser.add_argument("--device", choices=("cpu", "mps", "cuda"), required=True)
    parser.add_argument("--threads", type=int, required=True)
    args = parser.parse_args(namespace=DaggerArguments())
    if args.hands <= 0 or args.threads <= 0:
        parser.error("--hands and --threads must be positive")
    torch.set_num_threads(args.threads)
    student = load_policy(args.student, torch.device(args.device))
    env = Env(teacher=read_spec(args.teacher), rules=read_spec(args.rules))
    if student.metadata.dataset.rules_json != env.rules_json:
        parser.error("the student was trained under different rules")
    behavior = MixedBehavior(student, args.student_probability, np.random.default_rng(args.seed))
    dataset = collect_with(env, range(args.start_seed, args.start_seed + args.hands), behavior)
    save_dataset(args.output, dataset)
    provenance = CollectionProvenance(str(args.student), args.student_probability, args.seed, args.start_seed, args.hands)
    args.output.with_suffix(".collection.json").write_bytes(msgspec.json.encode(provenance))
    print(f"wrote {len(dataset.actions)} decisions from {args.hands} hands to {args.output}")


if __name__ == "__main__":
    main()
