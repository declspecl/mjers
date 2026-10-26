import math
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path

import numpy as np
import torch

from mjers._native import TILE_PLANES, Env
from mjers.collect import collect
from mjers.expert import (
    CollectionConfig,
    RefineConfig,
    RefineMetrics,
    collect_positions,
    concatenate,
    load_positions,
    refine,
    save_positions,
    search_targets,
    second_is_better,
)
from mjers.model import RESIDUAL_BLOCKS, PolicyMetadata, ResidualPolicyNet, TrainedPolicy, native_network


def student() -> TrainedPolicy:
    torch.manual_seed(5)
    network = ResidualPolicyNet(8, RESIDUAL_BLOCKS, TILE_PLANES)
    with torch.no_grad():
        for parameter in network.parameters():
            parameter.add_(0.05 * torch.randn_like(parameter))
    dataset = collect(Env("efficiency", rules="family"), range(0, 1))
    return TrainedPolicy(network.eval(), PolicyMetadata("suit-resnet-v2", 8, dataset.metadata, (0,), (1,)), torch.device("cpu"))


class ExpertTests(unittest.TestCase):
    def test_estimates_are_paired_deterministic_and_legal(self) -> None:
        env = Env("efficiency", rules="family")
        observation = env.reset(9, 1)
        assert observation is not None
        legal = [int(index) for index in np.flatnonzero(observation.action_mask)]
        network = student().network
        assert isinstance(network, ResidualPolicyNet)
        native = native_network(network)
        first = env.estimate_actions(native, legal[:2], samples=6, seed=3)
        self.assertEqual(first[0], (0.0, 0.0))
        self.assertEqual(first, env.estimate_actions(native, legal[:2], samples=6, seed=3))
        illegal = int(np.flatnonzero(~observation.action_mask)[0])
        with self.assertRaisesRegex(ValueError, "not legal"):
            env.estimate_actions(native, [legal[0], illegal], samples=2, seed=0)

    def test_posterior_shrinks_noisy_leads(self) -> None:
        leads = torch.tensor([0.5, 0.5, -0.2, 0.3, 0.0], dtype=torch.float64)
        errors = torch.tensor([0.0, 1e6, 0.0, 0.3, 0.0], dtype=torch.float64)
        better = second_is_better(leads, errors, 0.3)
        expected = 0.5 * (1 + math.erf(0.3 * 0.3 / (0.3 * math.sqrt(0.18)) / math.sqrt(2)))
        torch.testing.assert_close(better, torch.tensor([1.0, 0.5, 0.0, expected, 0.5], dtype=torch.float64))

    def test_collects_refines_and_records_seeds(self) -> None:
        policy = student()
        env = Env("efficiency", rules="family")
        config = CollectionConfig(samples=3, confident=1.0, anchors_per_hand=1, seed=2)
        parts = [collect_positions(env, policy, "student", range(start, start + 2), config) for start in (20, 22)]
        positions = concatenate(parts)
        self.assertTrue(positions.searched.any())
        self.assertLessEqual(int((~positions.searched).sum()), 4, "at most one anchor per hand")
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "positions.npz"
            save_positions(path, positions)
            restored = load_positions(path)
        np.testing.assert_array_equal(restored.leads, positions.leads)
        self.assertEqual(restored.metadata, positions.metadata)
        with self.assertRaisesRegex(ValueError, "repeat hand seeds"):
            concatenate([parts[0], parts[0]])

        base = torch.full((len(positions.seeds), 110), 0.0)
        base[torch.arange(len(base)), torch.from_numpy(positions.candidates[:, 0])] = 0.6
        base[torch.arange(len(base)), torch.from_numpy(positions.candidates[:, 1])] = 0.4
        targets = search_targets(base, positions, 0.3)
        torch.testing.assert_close(targets.sum(dim=1), torch.ones(len(base)))
        anchors = torch.from_numpy(~positions.searched)
        torch.testing.assert_close(targets[anchors], base[anchors])

        reports: list[RefineMetrics] = []
        refined = refine(policy, positions, RefineConfig(0.3, 1, 16, 1e-3, 2, 0, "cpu"), reports.append)
        self.assertEqual([report.epoch for report in reports], [0, 1])
        self.assertEqual(refined.metadata.training_seeds, (0, 20, 21, 22, 23))
        overlapping = replace(policy, metadata=replace(policy.metadata, validation_seeds=(21,)))
        with self.assertRaisesRegex(ValueError, "overlap"):
            refine(overlapping, positions, RefineConfig(0.3, 1, 16, 1e-3, 2, 0, "cpu"), reports.append)


if __name__ == "__main__":
    unittest.main()
