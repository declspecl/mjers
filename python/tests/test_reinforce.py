import unittest
from dataclasses import replace

import numpy as np
import torch

from mjers._native import ACTION_COUNT, CONTEXT_SIZE, PASS_ACTION, TILE_PLANES, Env
from mjers.collect import collect
from mjers.evaluate import compare
from mjers.model import PolicyMetadata, ResidualPolicyNet, TrainedPolicy
from mjers.reinforce import Experience, IterationReport, ReinforcementConfig, advantages, reinforce, update


def config(start_seed: int, snapshot_every: int = 0) -> ReinforcementConfig:
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
        claim_entropy_weight=0.01,
        gae_lambda=0.9,
        seed=4,
        start_seed=start_seed,
        device="cpu",
        threads=1,
        snapshot_every=snapshot_every,
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

    def test_self_play_records_its_snapshot_field(self) -> None:
        reports: list[IterationReport] = []
        policy, history = reinforce(self.policy(), "{}", "family", config(700, snapshot_every=1), lambda report, _: reports.append(report))
        self.assertEqual(len(history), 2)
        self.assertTrue(all(report.decisions > 0 for report in history))
        self.assertIn('"kind":"snapshot"', policy.metadata.reinforcement[-1].field_json)

    def test_claim_entropy_weight_opens_up_claim_responses(self) -> None:
        torch.manual_seed(3)
        network = ResidualPolicyNet(8)
        # Claim rows choose between pong and pass; the start strongly prefers passing.
        with torch.no_grad():
            network.other_actions.bias[PASS_ACTION - ACTION_COUNT] += 4.0
        mask = torch.zeros(8, ACTION_COUNT, dtype=torch.bool)
        mask[:4, [105, PASS_ACTION]] = True
        mask[4:, :3] = True
        tiles, context = torch.rand(8, TILE_PLANES, 34), torch.rand(8, CONTEXT_SIZE)
        actions = torch.tensor([PASS_ACTION] * 4 + [0] * 4)
        with torch.no_grad():
            log_probs = network.forward(tiles, context, mask).log_softmax(dim=1).gather(1, actions.unsqueeze(1)).squeeze(1)
        data = Experience(tiles, context, mask, actions, log_probs, torch.zeros(8), torch.zeros(8))

        def claim_entropy() -> float:
            with torch.no_grad():
                probabilities = network.forward(tiles[:4], context[:4], mask[:4]).softmax(dim=1)
            return float(-(probabilities * probabilities.clamp_min(1e-12).log()).sum(dim=1).mean())

        before = claim_entropy()
        settings = replace(config(0), value_weight=0.0, entropy_weight=0.0, claim_entropy_weight=1.0)
        update(network, torch.optim.Adam(network.parameters(), lr=1e-4), data, settings, torch.Generator().manual_seed(0))
        self.assertGreater(claim_entropy(), before)


if __name__ == "__main__":
    unittest.main()
