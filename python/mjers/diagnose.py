"""Explain validation disagreements using observation-only advisor assessments.

These are shape, live-tile and danger proxies, not counterfactual settlement EV.
Claims assume the advisor's best follow-up discard. The teacher is replayed
throughout: examples describe its state distribution rather than candidate play.
"""

import argparse
from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from typing import Literal, Protocol

import msgspec
import torch

from mjers._native import ACTION_COUNT, ActionAssessment, Env, Observation
from mjers.collect import read_spec
from mjers.model import DeviceName, PolicyMetadata, load_policy

type ActionKind = Literal["discard", "concealed_kong", "upgrade_kong", "chow", "pong", "kong", "win", "pass"]
type MistakeKind = Literal["discard_choice", "overclaim", "underclaim", "claim_choice", "missed_win", "other"]
type Consequence = Literal[
    "missed_win",
    "higher_shanten",
    "lower_shanten",
    "fewer_live_tiles",
    "more_live_tiles",
    "lower_payment",
    "higher_payment",
    "more_danger",
    "less_danger",
    "same_assessment",
]


class DecisionPolicy(Protocol):
    @property
    def metadata(self) -> PolicyMetadata: ...

    def act(self, observation: Observation) -> int: ...


@dataclass(frozen=True)
class ActionAnalysis:
    index: int
    description: str
    heuristic_rank: int
    shanten: int
    live_improving: int
    danger: float
    immediate_units: int
    reasons: tuple[str, ...]


@dataclass(frozen=True)
class Disagreement:
    seed: int
    step: int
    seat: int
    concealed_hand: str
    kind: MistakeKind
    consequence: Consequence
    teacher: ActionAnalysis
    candidate: ActionAnalysis


@dataclass(frozen=True)
class DecisionAccuracy:
    kind: ActionKind
    decisions: int
    matches: int


@dataclass(frozen=True)
class ErrorCount:
    kind: MistakeKind
    consequence: Consequence
    count: int


@dataclass(frozen=True)
class Diagnosis:
    rules_json: str
    teacher_json: str
    seeds: tuple[int, ...]
    decisions: int
    matches: int
    accuracy_by_teacher_action: tuple[DecisionAccuracy, ...]
    errors: tuple[ErrorCount, ...]
    examples_per_group: int
    examples: tuple[Disagreement, ...]


def action_kind(index: int) -> ActionKind:
    if not 0 <= index < ACTION_COUNT:
        raise ValueError("action index must be in 0..110")
    if index < 34:
        return "discard"
    if index < 68:
        return "concealed_kong"
    if index < 102:
        return "upgrade_kong"
    if index < 105:
        return "chow"
    if index == 105:
        return "pong"
    if index == 106:
        return "kong"
    if index in (107, 108):
        return "win"
    return "pass"


def mistake_kind(teacher: int, candidate: int) -> MistakeKind:
    expected, selected = action_kind(teacher), action_kind(candidate)
    claims = ("chow", "pong", "kong")
    if expected == "win":
        return "missed_win"
    if expected == "pass" and selected in claims:
        return "overclaim"
    if expected in claims and selected == "pass":
        return "underclaim"
    if expected in claims and selected in claims:
        return "claim_choice"
    if expected == "discard" and selected == "discard":
        return "discard_choice"
    return "other"


def consequence(teacher: ActionAnalysis, candidate: ActionAnalysis) -> Consequence:
    """First differing proxy; retain every raw measurement in the examples."""
    if action_kind(teacher.index) == "win" and action_kind(candidate.index) != "win":
        return "missed_win"
    if candidate.shanten > teacher.shanten:
        return "higher_shanten"
    if candidate.shanten < teacher.shanten:
        return "lower_shanten"
    if candidate.live_improving < teacher.live_improving:
        return "fewer_live_tiles"
    if candidate.live_improving > teacher.live_improving:
        return "more_live_tiles"
    if candidate.immediate_units < teacher.immediate_units:
        return "lower_payment"
    if candidate.immediate_units > teacher.immediate_units:
        return "higher_payment"
    if candidate.danger > teacher.danger + 1e-6:
        return "more_danger"
    if candidate.danger < teacher.danger - 1e-6:
        return "less_danger"
    return "same_assessment"


def analyze_action(ranked: list[ActionAssessment], index: int) -> ActionAnalysis:
    for rank, assessment in enumerate(ranked, start=1):
        if assessment.index == index:
            return ActionAnalysis(
                assessment.index,
                assessment.description,
                rank,
                assessment.shanten,
                assessment.live_improving,
                assessment.danger,
                assessment.immediate_units,
                tuple(assessment.reasons),
            )
    raise RuntimeError("legal action is missing from advisor assessments")


def diagnose(policy: DecisionPolicy, env: Env, examples_per_group: int) -> Diagnosis:
    if examples_per_group < 0:
        raise ValueError("examples per group must be nonnegative")
    dataset = policy.metadata.dataset
    if dataset.rules_json != env.rules_json or dataset.teacher_json != env.teacher_json:
        raise ValueError("diagnosis rules and teacher must match the checkpoint dataset")
    seeds = policy.metadata.validation_seeds
    if not seeds or set(seeds).intersection(policy.metadata.training_seeds):
        raise ValueError("diagnosis requires validation seeds disjoint from training")
    decisions_by_kind: Counter[ActionKind] = Counter()
    matches_by_kind: Counter[ActionKind] = Counter()
    errors_by_group: Counter[tuple[MistakeKind, Consequence]] = Counter()
    examples: list[Disagreement] = []
    for seed in seeds:
        observation = env.reset(seed, seed % 4)
        step = 0
        while observation is not None:
            # Exactly once per decision, including for stateful teachers.
            teacher = env.teacher_action()
            candidate = policy.act(observation)
            if not 0 <= candidate < ACTION_COUNT or not observation.action_mask[candidate]:
                raise ValueError("candidate returned an illegal action")
            kind = action_kind(teacher)
            decisions_by_kind[kind] += 1
            if teacher == candidate:
                matches_by_kind[kind] += 1
            else:
                ranked = env.rank_actions()
                expected = analyze_action(ranked, teacher)
                selected = analyze_action(ranked, candidate)
                mistake = mistake_kind(teacher, candidate)
                effect = consequence(expected, selected)
                group = (mistake, effect)
                errors_by_group[group] += 1
                if errors_by_group[group] <= examples_per_group:
                    examples.append(Disagreement(seed, step, observation.seat, env.concealed_hand, mistake, effect, expected, selected))
            observation = env.step(teacher).observation
            step += 1
    return Diagnosis(
        env.rules_json,
        env.teacher_json,
        seeds,
        sum(decisions_by_kind.values()),
        sum(matches_by_kind.values()),
        tuple(DecisionAccuracy(kind, count, matches_by_kind[kind]) for kind, count in sorted(decisions_by_kind.items())),
        tuple(ErrorCount(kind, effect, count) for (kind, effect), count in sorted(errors_by_group.items())),
        examples_per_group,
        tuple(examples),
    )


class DiagnoseArguments(argparse.Namespace):
    checkpoint: Path
    teacher: str
    rules: str
    examples_per_group: int
    device: DeviceName
    threads: int
    output: Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--checkpoint", type=Path, required=True)
    parser.add_argument("--teacher", required=True, help="preset or BotSpec JSON file")
    parser.add_argument("--rules", required=True, help="preset or Ruleset JSON file")
    parser.add_argument("--examples-per-group", type=int, required=True)
    parser.add_argument("--device", choices=("cpu", "mps", "cuda"), required=True)
    parser.add_argument("--threads", type=int, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(namespace=DiagnoseArguments())
    if args.threads <= 0:
        parser.error("--threads must be positive")
    torch.set_num_threads(args.threads)
    policy = load_policy(args.checkpoint, torch.device(args.device))
    env = Env(teacher=read_spec(args.teacher), rules=read_spec(args.rules))
    result = diagnose(policy, env, args.examples_per_group)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(msgspec.json.encode(result))
    print(msgspec.json.encode(result).decode())


if __name__ == "__main__":
    main()
