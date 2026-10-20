import unittest
from collections import Counter
from dataclasses import dataclass, replace

import msgspec
import numpy as np

from mjers._native import Env, Observation
from mjers.data import DatasetMetadata
from mjers.diagnose import ActionAnalysis, Diagnosis, action_kind, analyze_action, consequence, diagnose, mistake_kind
from mjers.model import PolicyMetadata


@dataclass(frozen=True)
class FirstLegalPolicy:
    metadata: PolicyMetadata

    def act(self, observation: Observation) -> int:
        return int(np.flatnonzero(observation.action_mask)[0])


def policy_for(env: Env, validation_seeds: tuple[int, ...]) -> FirstLegalPolicy:
    dataset = DatasetMetadata(1, env.rules_json, env.teacher_json)
    return FirstLegalPolicy(PolicyMetadata("mlp-relu-v1", 32, dataset, (0, 1), validation_seeds))


class DiagnosticsTests(unittest.TestCase):
    def test_rankings_cover_legal_actions_without_changing_teacher_rng_or_game(self) -> None:
        env = Env("random", rules="family")
        reference = Env("random", rules="family")
        observation = env.reset(8, 0)
        reference.reset(8, 0)
        while observation is not None:
            before = observation.tiles.copy()
            ranked = env.rank_actions()
            self.assertEqual({item.index for item in ranked}, set(np.flatnonzero(observation.action_mask)))
            self.assertTrue(env.concealed_hand)
            for item in ranked:
                self.assertTrue(item.reasons)
                self.assertGreaterEqual(item.live_improving, 0)
                self.assertGreaterEqual(item.danger, 0)
            fresh = env.observe()
            assert fresh is not None
            np.testing.assert_array_equal(before, fresh.tiles)
            action = env.teacher_action()
            self.assertEqual(action, reference.teacher_action())
            transition = env.step(action)
            expected = reference.step(action)
            self.assertEqual(transition.rewards, expected.rewards)
            observation = transition.observation
        with self.assertRaises(RuntimeError):
            env.rank_actions()
        with self.assertRaises(RuntimeError):
            _ = env.concealed_hand

    def test_diagnosis_replays_teacher_positions_and_bounds_examples_by_group(self) -> None:
        env = Env("efficiency", rules="family")
        policy = policy_for(env, (20, 21, 22))
        result = diagnose(policy, env, 1)
        self.assertEqual(result, msgspec.json.decode(msgspec.json.encode(result), type=Diagnosis))
        self.assertGreater(result.decisions, result.matches)
        self.assertEqual(sum(error.count for error in result.errors), result.decisions - result.matches)
        self.assertEqual(sum(group.decisions for group in result.accuracy_by_teacher_action), result.decisions)
        example_counts = Counter((example.kind, example.consequence) for example in result.examples)
        self.assertEqual(set(example_counts.values()), {1})
        self.assertEqual(len(result.examples), len(result.errors))
        no_examples = diagnose(policy, env, 0)
        self.assertEqual(no_examples.errors, result.errors)
        self.assertEqual(no_examples.matches, result.matches)
        self.assertEqual(no_examples.examples, ())

        decisions = 0
        matches = 0
        for seed in policy.metadata.validation_seeds:
            observation = env.reset(seed, seed % 4)
            step = 0
            while observation is not None:
                teacher = env.teacher_action()
                candidate = policy.act(observation)
                decisions += 1
                matches += teacher == candidate
                for example in result.examples:
                    if (example.seed, example.step) == (seed, step):
                        ranked = env.rank_actions()
                        self.assertEqual(example.seat, observation.seat)
                        self.assertEqual(example.concealed_hand, env.concealed_hand)
                        self.assertEqual(example.teacher, analyze_action(ranked, teacher))
                        self.assertEqual(example.candidate, analyze_action(ranked, candidate))
                observation = env.step(teacher).observation
                step += 1
        self.assertEqual((result.decisions, result.matches), (decisions, matches))

    def test_claim_and_shape_categories_describe_differences(self) -> None:
        self.assertEqual(mistake_kind(109, 102), "overclaim")
        self.assertEqual(mistake_kind(105, 109), "underclaim")
        self.assertEqual(mistake_kind(102, 105), "claim_choice")
        self.assertEqual(mistake_kind(0, 1), "discard_choice")
        self.assertEqual(mistake_kind(107, 109), "missed_win")
        teacher = ActionAnalysis(0, "discard 1m", 1, 1, 12, 0.1, 0, ())
        self.assertEqual(consequence(teacher, replace(teacher, shanten=2)), "higher_shanten")
        self.assertEqual(consequence(teacher, replace(teacher, shanten=0)), "lower_shanten")
        self.assertEqual(consequence(teacher, replace(teacher, live_improving=8)), "fewer_live_tiles")
        self.assertEqual(consequence(teacher, replace(teacher, live_improving=16)), "more_live_tiles")
        self.assertEqual(consequence(teacher, replace(teacher, danger=0.2)), "more_danger")
        self.assertEqual(consequence(teacher, replace(teacher, danger=0)), "less_danger")
        self.assertEqual(consequence(teacher, replace(teacher, immediate_units=3)), "higher_payment")
        self.assertEqual(consequence(replace(teacher, immediate_units=3), teacher), "lower_payment")
        self.assertEqual(consequence(teacher, replace(teacher, index=1)), "same_assessment")
        self.assertEqual(consequence(replace(teacher, index=107), replace(teacher, index=109)), "missed_win")
        with self.assertRaises(ValueError):
            action_kind(-1)
        with self.assertRaises(ValueError):
            action_kind(110)

    def test_diagnosis_rejects_mismatched_configuration_or_seen_training_seeds(self) -> None:
        env = Env("efficiency", rules="family")
        policy = policy_for(env, (20,))
        with self.assertRaisesRegex(ValueError, "match"):
            diagnose(policy, Env("random", rules="family"), 1)
        with self.assertRaisesRegex(ValueError, "match"):
            diagnose(policy, Env("efficiency", rules="discarder-pays"), 1)
        with self.assertRaisesRegex(ValueError, "disjoint"):
            diagnose(policy_for(env, (0,)), env, 1)
        with self.assertRaisesRegex(ValueError, "disjoint"):
            diagnose(policy_for(env, ()), env, 1)
        with self.assertRaisesRegex(ValueError, "nonnegative"):
            diagnose(policy, env, -1)


if __name__ == "__main__":
    unittest.main()
