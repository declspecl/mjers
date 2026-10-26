import tempfile
import unittest
from pathlib import Path

import numpy as np
import torch

from mjers._native import ACTION_COUNT, CONTEXT_SIZE, TILE_PLANES, Env
from mjers.collect import collect
from mjers.deepen import deepen
from mjers.model import (
    RESIDUAL_BLOCKS,
    PolicyMetadata,
    ResidualPolicyNet,
    SuitPolicyNet,
    TrainedPolicy,
    load_policy,
    native_network,
    save_policy,
)


class DeepenTests(unittest.TestCase):
    def policy(self) -> TrainedPolicy:
        torch.manual_seed(6)
        network = ResidualPolicyNet(8, RESIDUAL_BLOCKS, TILE_PLANES)
        # Perturb the zero-initialized branches so the original blocks all matter.
        with torch.no_grad():
            for parameter in network.parameters():
                parameter.add_(0.05 * torch.randn_like(parameter))
        dataset = collect(Env("efficiency", rules="family"), range(0, 1))
        return TrainedPolicy(network.eval(), PolicyMetadata("suit-resnet-v2", 8, dataset.metadata, (0,), (1,)), torch.device("cpu"))

    def test_added_blocks_leave_play_unchanged(self) -> None:
        policy = self.policy()
        deeper = deepen(policy, RESIDUAL_BLOCKS + 3)
        self.assertEqual(deeper.metadata.residual_blocks, RESIDUAL_BLOCKS + 3)
        tiles, context = torch.rand(5, TILE_PLANES, 34), torch.rand(5, CONTEXT_SIZE)
        mask = torch.ones(5, ACTION_COUNT, dtype=torch.bool)
        with torch.no_grad():
            expected = policy.network.heads(tiles, context, mask)
            actual = deeper.network.heads(tiles, context, mask)
        torch.testing.assert_close(actual, expected, rtol=0, atol=0)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "deeper.safetensors"
            save_policy(path, deeper)
            restored = load_policy(path, torch.device("cpu"))
        assert isinstance(restored.network, ResidualPolicyNet)
        self.assertEqual(len(restored.network.blocks), RESIDUAL_BLOCKS + 3)
        env = Env("efficiency", rules="family")
        observation = env.reset(4, 0)
        assert observation is not None
        logits, _settlement = env.evaluate_network(native_network(restored.network))
        with torch.no_grad():
            original = policy.network.forward(
                torch.from_numpy(observation.tiles).unsqueeze(0),
                torch.from_numpy(observation.context).unsqueeze(0),
                torch.from_numpy(observation.action_mask).unsqueeze(0),
            )
        legal = observation.action_mask
        np.testing.assert_allclose(np.asarray(logits, dtype=np.float32)[legal], original[0].numpy()[legal], rtol=1e-4, atol=1e-4)

    def test_rejects_other_architectures_and_shallower_targets(self) -> None:
        policy = self.policy()
        with self.assertRaisesRegex(ValueError, "more blocks"):
            deepen(policy, RESIDUAL_BLOCKS)
        cnn = TrainedPolicy(SuitPolicyNet(8), PolicyMetadata("suit-cnn-relu-v1", 8, policy.metadata.dataset, (0,), (1,)), policy.device)
        with self.assertRaisesRegex(ValueError, "suit-resnet-v2"):
            deepen(cnn, RESIDUAL_BLOCKS + 1)


if __name__ == "__main__":
    unittest.main()
