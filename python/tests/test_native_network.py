import unittest

import numpy as np
import torch

from mjers._native import Env, NeuralNet
from mjers.model import VALUE_SCALE, ResidualPolicyNet, native_network


class NativeNetworkTests(unittest.TestCase):
    def test_rust_inference_matches_pytorch_over_complete_hands(self) -> None:
        torch.manual_seed(8)
        network = ResidualPolicyNet(12)
        # Perturb the zero-initialized branches so every layer matters.
        with torch.no_grad():
            for parameter in network.parameters():
                parameter.add_(0.1 * torch.randn_like(parameter))
        network.eval()
        native = native_network(network)
        env = Env("efficiency", rules="family")
        decisions = 0
        for seed in range(3):
            observation = env.reset(seed, seed % 4)
            while observation is not None:
                logits, settlement = env.evaluate_network(native)
                with torch.inference_mode():
                    expected, value = network.heads(
                        torch.from_numpy(observation.tiles).unsqueeze(0),
                        torch.from_numpy(observation.context).unsqueeze(0),
                        torch.from_numpy(observation.action_mask).unsqueeze(0),
                    )
                actual = np.asarray(logits, dtype=np.float32)
                np.testing.assert_array_equal(np.isinf(actual), ~observation.action_mask)
                np.testing.assert_allclose(
                    actual[observation.action_mask], expected[0].numpy()[observation.action_mask], rtol=1e-4, atol=1e-4
                )
                self.assertAlmostEqual(settlement, float(value[0]) * VALUE_SCALE, places=3)
                decisions += 1
                observation = env.step(env.teacher_action()).observation
        self.assertGreater(decisions, 100)

    def test_incomplete_state_dicts_are_rejected(self) -> None:
        tensors = {name: tensor.numpy() for name, tensor in ResidualPolicyNet(4).state_dict().items()}
        del tensors["summary.bias"]
        with self.assertRaisesRegex(ValueError, "missing tensor summary.bias"):
            NeuralNet(tensors)


if __name__ == "__main__":
    unittest.main()
