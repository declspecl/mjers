import json
import tempfile
import unittest
from pathlib import Path

import numpy as np
import torch

from mjers._native import Env, NeuralNet
from mjers.collect import collect, make_env
from mjers.model import VALUE_SCALE, PolicyMetadata, ResidualPolicyNet, SuitPolicyNet, TrainedPolicy, native_network, save_policy


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


class NeuralTeacherTests(unittest.TestCase):
    def test_strategy_files_make_the_network_the_teacher(self) -> None:
        torch.manual_seed(3)
        network = ResidualPolicyNet(8)
        with torch.no_grad():
            for parameter in network.parameters():
                parameter.add_(0.1 * torch.randn_like(parameter))
        dataset = collect(Env("efficiency", rules="family"), range(0, 1))
        policy = TrainedPolicy(network.eval(), PolicyMetadata("suit-resnet-v2", 8, dataset.metadata, (0,), (1,)), torch.device("cpu"))
        with tempfile.TemporaryDirectory() as directory:
            checkpoint = Path(directory) / "policy.safetensors"
            save_policy(checkpoint, policy)
            strategy = Path(directory) / "strategy.json"
            strategy.write_text(json.dumps({"kind": "neural", "name": "net", "network": str(checkpoint)}))
            env = make_env(str(strategy), "family")
            self.assertIn(str(checkpoint), env.teacher_json)
            native = native_network(network)
            observation = env.reset(4, 0)
            while observation is not None:
                logits, _ = env.evaluate_network(native)
                self.assertEqual(env.teacher_action(), int(np.argmax(logits)))
                observation = env.step(env.teacher_action()).observation
            with self.assertRaisesRegex(ValueError, "different rules"):
                make_env(str(strategy), "discarder-pays")
        self.assertIn('"efficiency"', make_env("efficiency", "family").teacher_json)

    def test_only_residual_checkpoints_can_teach(self) -> None:
        dataset = collect(Env("efficiency", rules="family"), range(0, 1))
        policy = TrainedPolicy(
            SuitPolicyNet(8).eval(), PolicyMetadata("suit-cnn-relu-v1", 8, dataset.metadata, (0,), (1,)), torch.device("cpu")
        )
        with tempfile.TemporaryDirectory() as directory:
            checkpoint = Path(directory) / "cnn.safetensors"
            save_policy(checkpoint, policy)
            strategy = Path(directory) / "strategy.json"
            strategy.write_text(json.dumps({"kind": "neural", "name": "cnn", "network": str(checkpoint)}))
            with self.assertRaisesRegex(ValueError, "suit-resnet-v2"):
                make_env(str(strategy), "family")
