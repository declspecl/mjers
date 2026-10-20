import tempfile
import unittest
from pathlib import Path

import torch
from torch.optim import Adam

from mjers._native import ACTION_COUNT, CONTEXT_SIZE, TILE_PLANES, Env
from mjers.collect import collect
from mjers.model import PolicyNet, SuitPolicyNet, load_policy, save_policy
from mjers.train import EpochMetrics, PolicyBatch, TrainingConfig, learn_batch, train


class SuitNetworkTests(unittest.TestCase):
    def test_suits_share_filters_without_crossing_boundaries(self) -> None:
        torch.manual_seed(7)
        network = SuitPolicyNet(32)
        suit = torch.rand(2, TILE_PLANES, 9)
        honors = torch.rand(2, TILE_PLANES, 7)
        tiles = torch.cat((suit, suit, suit, honors), dim=2)
        features = network.tile_features(tiles)
        self.assertEqual(tuple(features.shape), (2, 48, 34))
        torch.testing.assert_close(features[:, :, :9], features[:, :, 9:18], rtol=0, atol=0)
        torch.testing.assert_close(features[:, :, :9], features[:, :, 18:27], rtol=0, atol=0)

        changed_suit = tiles.clone()
        changed_suit[:, :, 8] += 1
        changed_features = network.tile_features(changed_suit)
        self.assertFalse(torch.equal(features[:, :, :9], changed_features[:, :, :9]))
        torch.testing.assert_close(features[:, :, 9:], changed_features[:, :, 9:], rtol=0, atol=0)

        changed_honor = tiles.clone()
        changed_honor[:, :, 27] += 1
        changed_features = network.tile_features(changed_honor)
        self.assertFalse(torch.equal(features[:, :, 27], changed_features[:, :, 27]))
        torch.testing.assert_close(features[:, :, :27], changed_features[:, :, :27], rtol=0, atol=0)
        torch.testing.assert_close(features[:, :, 28:], changed_features[:, :, 28:], rtol=0, atol=0)

    def test_masked_gradients_reach_suit_honor_and_context_layers(self) -> None:
        torch.manual_seed(11)
        network = SuitPolicyNet(32)
        tiles = torch.rand(4, TILE_PLANES, 34)
        context = torch.rand(4, CONTEXT_SIZE)
        mask = torch.zeros(4, ACTION_COUNT, dtype=torch.bool)
        mask[:, :2] = True
        batch = PolicyBatch(tiles, context, mask, torch.tensor([0, 1, 0, 1]))
        logits = network.forward(tiles, context, mask)
        self.assertEqual(tuple(logits.shape), (4, ACTION_COUNT))
        self.assertTrue(bool(torch.isfinite(logits[:, :2]).all()))
        self.assertTrue(bool(torch.isneginf(logits[:, 2:]).all()))
        before = network.suit_input.weight.detach().clone()
        learn_batch(network, Adam(network.parameters(), lr=0.001), batch)
        self.assertFalse(torch.equal(before, network.suit_input.weight))
        for name, parameter in network.named_parameters():
            with self.subTest(parameter=name):
                gradient = parameter.grad
                self.assertIsNotNone(gradient)
                assert gradient is not None
                self.assertTrue(bool(torch.isfinite(gradient).all()))
                self.assertGreater(int(gradient.count_nonzero().item()), 0)
        action_gradient = network.action_layer.bias.grad
        assert action_gradient is not None
        self.assertEqual(int(action_gradient[2:].count_nonzero().item()), 0)

    def test_training_selects_and_restores_cnn_checkpoint(self) -> None:
        training = collect(Env("efficiency", rules="family"), range(0, 8))
        validation = collect(Env("efficiency", rules="family"), range(100, 104))
        config = TrainingConfig("suit-cnn-relu-v1", 32, 4, 64, 0.003, 7, "cpu", 1)
        epochs: list[EpochMetrics] = []
        result = train(training, validation, config, epochs.append)
        self.assertIsInstance(result.policy.network, SuitPolicyNet)
        self.assertEqual(result.policy.metadata.architecture, "suit-cnn-relu-v1")
        self.assertEqual(len(epochs), config.epochs + 1)
        self.assertLess(epochs[-1].training.loss, epochs[0].training.loss)
        self.assertEqual(epochs[result.report.best_epoch].validation.loss, min(epoch.validation.loss for epoch in epochs))
        batch = PolicyBatch.from_dataset(validation, torch.device("cpu"))
        expected = result.policy.network.forward(batch.tiles, batch.context, batch.action_mask)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "cnn.safetensors"
            save_policy(path, result.policy)
            restored = load_policy(path, torch.device("cpu"))
            self.assertIsInstance(restored.network, SuitPolicyNet)
            self.assertEqual(restored.metadata, result.policy.metadata)
            actual = restored.network.forward(batch.tiles, batch.context, batch.action_mask)
            torch.testing.assert_close(expected, actual, rtol=0, atol=0)
            observation = Env("efficiency", rules="family").reset(200, 0)
            assert observation is not None
            self.assertTrue(observation.action_mask[restored.act(observation)])

    def test_parameter_budget_matches_mlp_and_width_is_required_positive(self) -> None:
        mlp_count = sum(parameter.numel() for parameter in PolicyNet(256).parameters())
        cnn_count = sum(parameter.numel() for parameter in SuitPolicyNet(256).parameters())
        self.assertEqual(mlp_count, 535150)
        self.assertEqual(cnn_count, 536750)
        for width in (0, -1):
            with self.subTest(width=width), self.assertRaisesRegex(ValueError, "positive"):
                SuitPolicyNet(width)


if __name__ == "__main__":
    unittest.main()
