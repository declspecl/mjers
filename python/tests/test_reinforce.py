import unittest

import numpy as np
import torch

from mjers._native import Env
from mjers.collect import collect
from mjers.evaluate import compare
from mjers.model import PolicyMetadata, ResidualPolicyNet, TrainedPolicy
from mjers.reinforce import IterationReport, ReinforcementConfig, advantages, reinforce


def config(start_seed: int) -> ReinforcementConfig:
    return ReinforcementConfig(
        iterations=2,
        hands_per_iteration=6,
        environments=3,
        epochs=2,
        minibatch_size=32,
        learning_rate=0.001,
        clip=0.2,
        value_weight=0.5,
        entropy_weight=0.01,
        gae_lambda=0.9,
        seed=4,
        start_seed=start_seed,
        device="cpu",
        threads=1,
    )


class ReinforcementTests(unittest.TestCase):
    def policy(self) -> TrainedPolicy:
        torch.manual_seed(2)
        dataset = collect(Env("efficiency", rules="family"), range(0, 1))
        metadata = PolicyMetadata("suit-resnet-v2", 8, dataset.metadata, (0,), (1,))
        return TrainedPolicy(ResidualPolicyNet(8).eval(), metadata, torch.device("cpu"))

    def test_advantages_accumulate_the_terminal_reward(self) -> None:
        values = np.asarray([0.5, 0.25], dtype=np.float32)
        np.testing.assert_allclose(advantages(values, 1.0, 1.0), [0.5, 0.75])
        np.testing.assert_allclose(advantages(values, 1.0, 0.0), [-0.25, 0.75])

    def test_training_records_its_seeds_and_changes_the_policy(self) -> None:
        initial = self.policy()
        before = {name: tensor.clone() for name, tensor in initial.network.state_dict().items()}
        env = Env("efficiency", rules="family")
        reports: list[IterationReport] = []
        snapshots: list[range] = []

        def record(report: IterationReport, snapshot: TrainedPolicy) -> None:
            reports.append(report)
            snapshots.append(snapshot.metadata.reinforcement[-1].seeds())

        policy, history = reinforce(initial, env.teacher_json, "family", config(500), record)
        self.assertEqual(snapshots, [range(500, 506), range(500, 512)])
        self.assertEqual(len(history), 2)
        self.assertEqual(reports, list(history))
        self.assertTrue(all(report.hands == 6 and report.decisions > 0 for report in history))
        self.assertTrue(all(np.isfinite([report.entropy, report.approximate_kl, report.value_loss]).all() for report in history))
        self.assertEqual(policy.metadata.reinforcement[-1].seeds(), range(500, 512))
        after = policy.network.state_dict()
        self.assertTrue(any(not torch.equal(before[name], after[name]) for name in before))
        with self.assertRaisesRegex(ValueError, "overlap"):
            compare(policy, env, range(510, 514), "all")
        with self.assertRaisesRegex(ValueError, "overlap"):
            reinforce(policy, env.teacher_json, "family", config(505), record)
        self.assertEqual(compare(policy, env, range(600, 602), "all").seeds, (600, 601))


if __name__ == "__main__":
    unittest.main()
