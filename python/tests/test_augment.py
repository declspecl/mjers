import unittest

import torch
from torch import Tensor

from mjers._native import Env
from mjers.augment import CHOW_PLANES, Relabeling, random_relabeling, relabel_actions, relabel_mask, relabel_tiles, relabeling
from mjers.collect import collect

OWN = 0
PUBLIC = 4
OFFERED = 48
TURN = 16
FROM_PREVIOUS = 15
DISCARD_OFFER = 17


def chow_legality(tiles: Tensor, context: Tensor) -> Tensor:
    """Chow legality implied by the encoded hand and offered tile alone."""
    offered = tiles[:, OFFERED].argmax(dim=1)
    held = tiles[:, OWN] > 0
    legal = torch.zeros(len(tiles), 3, dtype=torch.bool)
    for row in range(len(tiles)):
        if context[row, FROM_PREVIOUS] == 0 or context[row, DISCARD_OFFER] == 0:
            continue
        tile = int(offered[row])
        if tile >= 27:
            continue
        for position in range(3):
            lowest = tile - position
            if lowest < 0 or lowest // 9 != tile // 9 or lowest % 9 > 6:
                continue
            others = [lowest + i for i in range(3) if lowest + i != tile]
            legal[row, position] = bool(held[row, others].all())
    return legal


def identity(batch_size: int) -> Relabeling:
    return relabeling(
        torch.arange(3).expand(batch_size, -1), torch.zeros(batch_size, dtype=torch.bool), torch.arange(7).expand(batch_size, -1)
    )


class AugmentationTests(unittest.TestCase):
    tiles: Tensor
    context: Tensor
    mask: Tensor
    actions: Tensor

    @classmethod
    def setUpClass(cls) -> None:
        dataset = collect(Env("efficiency", rules="family"), range(0, 40))
        cls.tiles = torch.from_numpy(dataset.tiles)
        cls.context = torch.from_numpy(dataset.context)
        cls.mask = torch.from_numpy(dataset.action_mask)
        cls.actions = torch.from_numpy(dataset.actions)

    def check_consistent(self, tiles: Tensor, mask: Tensor, actions: Tensor) -> None:
        self.assertTrue(bool(mask[torch.arange(len(actions)), actions].all()), "teacher actions stay legal")
        turns = self.context[:, TURN] > 0
        torch.testing.assert_close(mask[turns, :34], tiles[turns, OWN] > 0, rtol=0, atol=0)
        claims = ~turns
        torch.testing.assert_close(mask[claims, 102:105], chow_legality(tiles[claims], self.context[claims]), rtol=0, atol=0)
        public = tiles[:, PUBLIC] > 0
        for plane in CHOW_PLANES:
            rows, anchors = (tiles[:, plane] > 0).nonzero(as_tuple=True)
            self.assertTrue(bool((anchors < 27).all() and (anchors % 9 <= 6).all()))
            for offset in range(3):
                self.assertTrue(bool(public[rows, anchors + offset].all()), "chow tiles are public")

    def test_recorded_decisions_satisfy_the_checks(self) -> None:
        self.assertGreater(int((self.tiles[:, list(CHOW_PLANES)] > 0).sum()), 0, "data includes chow melds")
        self.assertGreater(int(self.mask[:, 102:105].sum()), 0, "data includes chow decisions")
        self.check_consistent(self.tiles, self.mask, self.actions)

    def test_identity_changes_nothing(self) -> None:
        same = identity(len(self.actions))
        torch.testing.assert_close(relabel_tiles(self.tiles, same), self.tiles, rtol=0, atol=0)
        torch.testing.assert_close(relabel_mask(self.mask, same), self.mask, rtol=0, atol=0)
        torch.testing.assert_close(relabel_actions(self.actions, same), self.actions, rtol=0, atol=0)

    def test_random_relabelings_preserve_legality_and_board_semantics(self) -> None:
        generator = torch.Generator().manual_seed(3)
        for trial in range(4):
            with self.subTest(trial=trial):
                relabeled = random_relabeling(len(self.actions), generator, torch.device("cpu"))
                self.assertGreater(int(relabeled.reversed.sum()), 0)
                tiles = relabel_tiles(self.tiles, relabeled)
                mask = relabel_mask(self.mask, relabeled)
                actions = relabel_actions(self.actions, relabeled)
                self.check_consistent(tiles, mask, actions)
                torch.testing.assert_close(tiles.sum(dim=2), self.tiles.sum(dim=2))
                self.assertEqual(int(mask.sum()), int(self.mask.sum()))

    def test_reversal_moves_chow_anchor_and_offered_position(self) -> None:
        tiles = torch.zeros(1, 50, 34)
        tiles[0, CHOW_PLANES[1], 9 + 1] = 0.25  # 234p chow by the next seat
        tiles[0, OFFERED, 2] = 1.0  # offered 3m
        mask = torch.zeros(1, 110, dtype=torch.bool)
        mask[0, 102] = True  # 3m as the low tile: 345m
        reverse_and_swap = relabeling(torch.tensor([[1, 0, 2]]), torch.tensor([True]), torch.arange(7).unsqueeze(0))
        relabeled = relabel_tiles(tiles, reverse_and_swap)
        # 234p becomes 678m (anchor 6m); 3m becomes 7p; 345m becomes 567p, offered as the high tile.
        self.assertEqual(relabeled[0, CHOW_PLANES[1]].nonzero().flatten().tolist(), [5])
        self.assertEqual(relabeled[0, OFFERED].nonzero().flatten().tolist(), [9 + 6])
        self.assertEqual(relabel_mask(mask, reverse_and_swap)[0].nonzero().flatten().tolist(), [104])
        self.assertEqual(
            relabel_actions(
                torch.tensor([102, 2, 36, 109]),
                relabeling(torch.tensor([[1, 0, 2]] * 4), torch.tensor([True] * 4), torch.arange(7).expand(4, -1)),
            ).tolist(),
            [104, 9 + 6, 34 + 9 + 6, 109],
        )


if __name__ == "__main__":
    unittest.main()
