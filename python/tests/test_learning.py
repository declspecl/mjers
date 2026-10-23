import tempfile
import unittest
from dataclasses import replace
from pathlib import Path

import numpy as np
import torch

from mjers._native import ACTION_COUNT, Env
from mjers.collect import collect
from mjers.data import concatenate, load_dataset, load_datasets, save_dataset
from mjers.evaluate import compare
from mjers.model import load_policy, save_policy
from mjers.train import EpochMetrics, TrainingConfig, TrainingResult, train


class LearningTests(unittest.TestCase):
    def test_training_checkpoint_and_complete_hand_evaluation(self) -> None:
        training = collect(Env("efficiency", rules="family"), range(0, 8))
        validation = collect(Env("efficiency", rules="family"), range(100, 104))
        config = TrainingConfig(
            architecture="mlp-relu-v1",
            hidden_width=32,
            epochs=8,
            batch_size=64,
            learning_rate=0.01,
            seed=7,
            device="cpu",
            threads=1,
            augment="symmetry",
            value_weight=0.0,
            soft_teacher=None,
        )
        epochs: list[EpochMetrics] = []
        best: list[TrainingResult] = []
        result = train(training, validation, config, epochs.append, best.append)
        self.assertEqual(len(epochs), config.epochs + 1)
        self.assertLess(result.report.history[-1].training.loss, result.report.history[0].training.loss)
        self.assertGreater(result.report.best_epoch, 0)

        with tempfile.TemporaryDirectory() as directory:
            shard_path = Path(directory) / "decisions.npz"
            save_dataset(shard_path, training)
            restored = load_dataset(shard_path)
            self.assertEqual(restored.metadata, training.metadata)
            np.testing.assert_array_equal(restored.tiles, training.tiles)
            np.testing.assert_array_equal(restored.actions, training.actions)
            np.testing.assert_array_equal(restored.returns, training.returns)
            checkpoint_path = Path(directory) / "policy.safetensors"
            save_policy(checkpoint_path, result.policy)
            loaded = load_policy(checkpoint_path, torch.device("cpu"))
            self.assertEqual(loaded.metadata, result.policy.metadata)
            env = Env("efficiency", rules="family")
            observation = env.reset(200, 0)
            assert observation is not None
            self.assertEqual(result.policy.act(observation), loaded.act(observation))
            logits = loaded.network.forward(
                torch.tensor(observation.tiles).unsqueeze(0),
                torch.tensor(observation.context).unsqueeze(0),
                torch.tensor(observation.action_mask).unsqueeze(0),
            )
            self.assertEqual(tuple(logits.shape), (1, ACTION_COUNT))
            illegal = torch.tensor(~observation.action_mask).unsqueeze(0)
            self.assertTrue(bool(torch.isneginf(logits[illegal]).all()))
            self.assertTrue(bool(torch.isfinite(logits[~illegal]).all()))
            comparison = compare(loaded, env, range(200, 202), "all")
            self.assertEqual(comparison.seeds, (200, 201))
            with self.assertRaisesRegex(ValueError, "overlap"):
                compare(loaded, env, range(100, 102), "all")
            with self.assertRaisesRegex(ValueError, "rules"):
                compare(loaded, Env("efficiency", rules="discarder-pays"), range(200, 202), "all")

    def test_training_rejects_leaking_seeds_or_mixed_rules(self) -> None:
        training = collect(Env("efficiency", rules="family"), range(0, 2))
        config = TrainingConfig("mlp-relu-v1", 32, 1, 32, 0.001, 0, "cpu", 1, "none", 0.0, None)
        epochs: list[EpochMetrics] = []
        best: list[TrainingResult] = []
        with self.assertRaisesRegex(ValueError, "overlap"):
            train(training, training, config, epochs.append, best.append)
        other = collect(Env("efficiency", rules="discarder-pays"), range(10, 12))
        with self.assertRaisesRegex(ValueError, "rules"):
            train(training, other, config, epochs.append, best.append)
        self.assertEqual(epochs, [])

    def test_dataset_rejects_invalid_labels_shapes_and_encoding(self) -> None:
        dataset = collect(Env("efficiency", rules="family"), range(0, 2))
        invalid = dataset.actions.copy()
        invalid[0] = ACTION_COUNT
        with self.assertRaisesRegex(ValueError, "out-of-range"):
            replace(dataset, actions=invalid)
        mask = dataset.action_mask.copy()
        mask[0, dataset.actions[0]] = False
        with self.assertRaisesRegex(ValueError, "illegal"):
            replace(dataset, action_mask=mask)
        with self.assertRaisesRegex(ValueError, "dimensions"):
            replace(dataset, tiles=dataset.tiles[:, :1])
        with self.assertRaisesRegex(ValueError, "encoding"):
            replace(dataset, metadata=replace(dataset.metadata, encoding_version=999))

    def test_concatenation_joins_disjoint_shards_only(self) -> None:
        first = collect(Env("efficiency", rules="family"), range(0, 2))
        second = collect(Env("efficiency", rules="family"), range(2, 4))
        joined = concatenate([first, second])
        self.assertEqual(len(joined.actions), len(first.actions) + len(second.actions))
        np.testing.assert_array_equal(joined.seeds[: len(first.seeds)], first.seeds)
        with self.assertRaisesRegex(ValueError, "repeat"):
            concatenate([first, first])
        with self.assertRaisesRegex(ValueError, "rules"):
            concatenate([first, collect(Env("efficiency", rules="discarder-pays"), range(4, 6))])

    def test_streamed_loading_matches_concatenation(self) -> None:
        shards = [collect(Env("efficiency", rules="family"), range(start, start + 2)) for start in (0, 2, 4)]
        with tempfile.TemporaryDirectory() as directory:
            paths = [Path(directory) / f"shard-{i}.npz" for i in range(3)]
            for path, shard in zip(paths, shards, strict=True):
                save_dataset(path, shard)
            streamed = load_datasets(paths)
            joined = concatenate(shards)
            self.assertEqual(streamed.metadata, joined.metadata)
            for name in ("tiles", "context", "action_mask", "actions", "returns", "seats", "seeds"):
                np.testing.assert_array_equal(getattr(streamed, name), getattr(joined, name))
            with self.assertRaisesRegex(ValueError, "repeat"):
                load_datasets([paths[0], paths[0]])
            other = Path(directory) / "other.npz"
            save_dataset(other, collect(Env("efficiency", rules="discarder-pays"), range(10, 12)))
            with self.assertRaisesRegex(ValueError, "rules"):
                load_datasets([paths[0], other])


if __name__ == "__main__":
    unittest.main()
