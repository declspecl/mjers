import tempfile
import unittest
from dataclasses import replace
from pathlib import Path

import numpy as np
import torch

from mjers._native import ENCODING_VERSION, TILE_PLANES, V1_TILE_PLANES, Env
from mjers.collect import collect
from mjers.model import RESIDUAL_BLOCKS, PolicyMetadata, ResidualPolicyNet, TrainedPolicy, load_policy, native_network, save_policy
from mjers.upgrade import upgrade


def version_one_policy() -> TrainedPolicy:
    torch.manual_seed(4)
    network = ResidualPolicyNet(8, RESIDUAL_BLOCKS, V1_TILE_PLANES)
    with torch.no_grad():
        for parameter in network.parameters():
            parameter.add_(0.05 * torch.randn_like(parameter))
    dataset = collect(Env("efficiency", rules="family"), range(0, 1))
    metadata = PolicyMetadata("suit-resnet-v2", 8, replace(dataset.metadata, encoding_version=1), (0,), (1,))
    return TrainedPolicy(network.eval(), metadata, torch.device("cpu"))


class UpgradeTests(unittest.TestCase):
    def test_upgraded_policies_play_the_same_and_read_current_planes(self) -> None:
        policy = version_one_policy()
        upgraded = upgrade(policy)
        assert isinstance(upgraded.network, ResidualPolicyNet)
        self.assertEqual(upgraded.network.planes, TILE_PLANES)
        self.assertEqual(upgraded.metadata.dataset.encoding_version, ENCODING_VERSION)
        self.assertEqual(upgraded.metadata.training_seeds, policy.metadata.training_seeds)
        env = Env("efficiency", rules="family")
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "upgraded.safetensors"
            save_policy(path, upgraded)
            restored = load_policy(path, torch.device("cpu"))
        assert isinstance(restored.network, ResidualPolicyNet)
        native = native_network(restored.network)
        observation = env.reset(6, 2)
        while observation is not None:
            inputs = [torch.from_numpy(x).unsqueeze(0) for x in (observation.tiles, observation.context, observation.action_mask)]
            with torch.no_grad():
                expected = policy.network.heads(*inputs)
                actual = restored.network.heads(*inputs)
            for a, b in zip(actual, expected, strict=True):
                torch.testing.assert_close(a, b, rtol=1e-6, atol=1e-6)
            logits, _ = env.evaluate_network(native)
            legal = observation.action_mask
            np.testing.assert_allclose(np.asarray(logits, dtype=np.float32)[legal], expected[0][0].numpy()[legal], rtol=1e-4, atol=1e-4)
            observation = env.step(env.teacher_action()).observation
        with self.assertRaisesRegex(ValueError, "already reads"):
            upgrade(upgraded)


if __name__ == "__main__":
    unittest.main()
