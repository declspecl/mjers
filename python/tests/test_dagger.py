import unittest

import numpy as np
import torch

from mjers._native import Env
from mjers.collect import collect, collect_with
from mjers.dagger import MixedBehavior
from mjers.model import PolicyMetadata, SuitPolicyNet, TrainedPolicy


class DaggerTests(unittest.TestCase):
    def student(self) -> TrainedPolicy:
        torch.manual_seed(1)
        dataset = collect(Env("efficiency", rules="family"), range(0, 1))
        metadata = PolicyMetadata("suit-cnn-relu-v1", 16, dataset.metadata, (0,), (1,))
        return TrainedPolicy(SuitPolicyNet(16).eval(), metadata, torch.device("cpu"))

    def test_teacher_only_behavior_matches_collection(self) -> None:
        env = Env("efficiency", rules="family")
        mixed = collect_with(env, range(10, 13), MixedBehavior(self.student(), 0.0, np.random.default_rng(0)))
        plain = collect(env, range(10, 13))
        np.testing.assert_array_equal(mixed.tiles, plain.tiles)
        np.testing.assert_array_equal(mixed.actions, plain.actions)
        np.testing.assert_array_equal(mixed.returns, plain.returns)

    def test_student_play_reaches_new_positions_with_teacher_labels(self) -> None:
        env = Env("efficiency", rules="family")
        mixed = collect_with(env, range(10, 13), MixedBehavior(self.student(), 1.0, np.random.default_rng(0)))
        plain = collect(env, range(10, 13))
        self.assertTrue(len(mixed.actions) != len(plain.actions) or not np.array_equal(mixed.tiles, plain.tiles))
        self.assertTrue(bool(mixed.action_mask[np.arange(len(mixed.actions)), mixed.actions].all()))
        with self.assertRaisesRegex(ValueError, "probability"):
            MixedBehavior(self.student(), 1.5, np.random.default_rng(0))


if __name__ == "__main__":
    unittest.main()
