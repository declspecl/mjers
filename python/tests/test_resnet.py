import tempfile
import unittest
from pathlib import Path

import torch
from torch.optim import Adam

from mjers._native import ACTION_COUNT, CONTEXT_SIZE, TILE_PLANES, Env
from mjers.augment import relabel_mask, relabel_tiles, relabeling
from mjers.collect import collect
from mjers.model import ResidualPolicyNet, load_policy, save_policy
from mjers.train import EpochMetrics, PolicyBatch, TrainingConfig, learn_batch, train


class ResidualNetworkTests(unittest.TestCase):
    def test_tile_logits_follow_suit_and_honor_permutations(self) -> None:
        torch.manual_seed(5)
        network = ResidualPolicyNet(16)
        # Perturb the zero-initialized branches so every block contributes.
        with torch.no_grad():
            for parameter in network.parameters():
                parameter.add_(0.05 * torch.randn_like(parameter))
        tiles = torch.rand(3, TILE_PLANES, 34)
        context = torch.rand(3, CONTEXT_SIZE)
        mask = torch.ones(3, ACTION_COUNT, dtype=torch.bool)
        permuted = relabeling(torch.tensor([[2, 0, 1]] * 3), torch.zeros(3, dtype=torch.bool), torch.tensor([[6, 5, 4, 3, 2, 1, 0]] * 3))
        logits, value = network.heads(tiles, context, mask)
        moved_logits, moved_value = network.heads(relabel_tiles(tiles, permuted), context, mask)
        torch.testing.assert_close(moved_logits, relabel_mask(logits, permuted), rtol=1e-5, atol=1e-5)
        torch.testing.assert_close(moved_value, value, rtol=1e-5, atol=1e-5)

    def test_value_loss_trains_every_parameter(self) -> None:
        torch.manual_seed(9)
        network = ResidualPolicyNet(16)
        tiles = torch.rand(4, TILE_PLANES, 34)
        context = torch.rand(4, CONTEXT_SIZE)
        mask = torch.zeros(4, ACTION_COUNT, dtype=torch.bool)
        mask[:, [0, 109]] = True
        batch = PolicyBatch(tiles, context, mask, torch.tensor([0, 109, 0, 109]), torch.tensor([9.0, -3.0, 0.0, -6.0]))
        optimizer = Adam(network.parameters(), lr=0.01)
        # The zero-initialized second convolutions pass no gradient on the first step.
        learn_batch(network, optimizer, batch, 1.0)
        learn_batch(network, optimizer, batch, 1.0)
        for name, parameter in network.named_parameters():
            with self.subTest(parameter=name):
                gradient = parameter.grad
                assert gradient is not None
                self.assertTrue(bool(torch.isfinite(gradient).all()))
                self.assertGreater(int(gradient.count_nonzero().item()), 0)

    def test_training_reports_value_loss_and_restores_checkpoint(self) -> None:
        training = collect(Env("efficiency", rules="family"), range(0, 8))
        validation = collect(Env("efficiency", rules="family"), range(100, 104))
        config = TrainingConfig("suit-resnet-v2", 16, 3, 64, 0.003, 7, "cpu", 1, "symmetry", 0.5)
        epochs: list[EpochMetrics] = []
        result = train(training, validation, config, epochs.append)
        self.assertIsInstance(result.policy.network, ResidualPolicyNet)
        value_losses = [epoch.validation.value_loss for epoch in epochs]
        self.assertTrue(all(loss is not None and loss >= 0 for loss in value_losses))
        self.assertLess(epochs[-1].training.loss, epochs[0].training.loss)
        batch = PolicyBatch.from_dataset(validation, torch.device("cpu"))
        expected = result.policy.network.forward(batch.tiles, batch.context, batch.action_mask)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "resnet.safetensors"
            save_policy(path, result.policy)
            restored = load_policy(path, torch.device("cpu"))
            self.assertIsInstance(restored.network, ResidualPolicyNet)
            actual = restored.network.forward(batch.tiles, batch.context, batch.action_mask)
            torch.testing.assert_close(expected, actual, rtol=0, atol=0)

    def test_value_weight_requires_a_value_head(self) -> None:
        with self.assertRaisesRegex(ValueError, "value head"):
            TrainingConfig("suit-cnn-relu-v1", 16, 1, 64, 0.003, 7, "cpu", 1, "none", 0.5)
        with self.assertRaisesRegex(ValueError, "nonnegative"):
            TrainingConfig("suit-resnet-v2", 16, 1, 64, 0.003, 7, "cpu", 1, "none", -1.0)


if __name__ == "__main__":
    unittest.main()
