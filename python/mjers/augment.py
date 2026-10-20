"""Tile relabelings that leave every current ruleset unchanged.

No rule or payout distinguishes the three suits, the direction ranks are read,
or individual honors. Permuting suits, reversing ranks within every suit, and
permuting honors therefore maps a position to one of equal value whose correct
action is the relabeled action. Encoding version 1 anchors chow melds at their
lowest tile and names chow actions by the offered tile's place in the sequence;
reversal moves both, so they are remapped separately.
"""

from dataclasses import dataclass

import torch
from torch import Tensor

from mjers._native import ACTION_COUNT, TILE_PLANES

TILE_KINDS = 34
SUITED_KINDS = 27
HONOR_KINDS = 7
TILE_ACTION_BLOCKS = 3
CHOW_ACTIONS = 102
# Chow meld anchor plane per relative seat (encoding version 1).
CHOW_PLANES = (32, 36, 40, 44)
# Index of the zero column used for chow anchors that reversal leaves empty.
EMPTY = TILE_KINDS


@dataclass(frozen=True)
class Relabeling:
    """Per-decision tile maps; new tile `j` shows old tile `source[:, j]`."""

    source: Tensor
    chow_source: Tensor
    target: Tensor
    reversed: Tensor


def relabeling(suits: Tensor, reversed: Tensor, honors: Tensor) -> Relabeling:
    """Build maps from suit and honor permutations ([B, 3], [B, 7]) and reversal flags ([B])."""
    batch_size = suits.shape[0]
    if suits.shape != (batch_size, 3) or honors.shape != (batch_size, HONOR_KINDS) or reversed.shape != (batch_size,):
        raise ValueError("relabeling needs one suit permutation, honor permutation, and reversal flag per decision")
    ranks = torch.arange(9, device=suits.device)
    read = torch.where(reversed.unsqueeze(1), 8 - ranks, ranks)
    suited = (suits.unsqueeze(2) * 9 + read.unsqueeze(1)).reshape(batch_size, SUITED_KINDS)
    source = torch.cat((suited, SUITED_KINDS + honors), dim=1)
    # A chow anchored at rank r covers r..r+2; reversed, its lowest rank is 6 - r.
    anchor = torch.where(reversed.unsqueeze(1), 6 - ranks, ranks)
    anchored = suits.unsqueeze(2) * 9 + anchor.unsqueeze(1)
    anchored = torch.where((anchor >= 0).unsqueeze(1), anchored, EMPTY).reshape(batch_size, SUITED_KINDS)
    chow_source = torch.cat((anchored, SUITED_KINDS + honors), dim=1)
    target = torch.empty_like(source).scatter_(1, source, torch.arange(TILE_KINDS, device=source.device).expand(batch_size, -1))
    return Relabeling(source, chow_source, target, reversed)


def random_relabeling(batch_size: int, generator: torch.Generator, device: torch.device) -> Relabeling:
    suits = torch.rand(batch_size, 3, generator=generator).argsort(dim=1)
    honors = torch.rand(batch_size, HONOR_KINDS, generator=generator).argsort(dim=1)
    reversed = torch.rand(batch_size, generator=generator) < 0.5
    return relabeling(suits.to(device), reversed.to(device), honors.to(device))


def relabel_tiles(tiles: Tensor, relabeling: Relabeling) -> Tensor:
    padded = torch.cat((tiles, tiles.new_zeros(tiles.shape[0], TILE_PLANES, 1)), dim=2)
    relabeled = padded.gather(2, relabeling.source.unsqueeze(1).expand(-1, TILE_PLANES, -1))
    chows = list(CHOW_PLANES)
    anchors = padded[:, chows].gather(2, relabeling.chow_source.unsqueeze(1).expand(-1, len(chows), -1))
    relabeled[:, chows] = anchors
    return relabeled


def relabel_mask(mask: Tensor, relabeling: Relabeling) -> Tensor:
    blocks = mask[:, :CHOW_ACTIONS].reshape(-1, TILE_ACTION_BLOCKS, TILE_KINDS)
    blocks = blocks.gather(2, relabeling.source.unsqueeze(1).expand(-1, TILE_ACTION_BLOCKS, -1))
    chows = mask[:, CHOW_ACTIONS : CHOW_ACTIONS + 3]
    chows = torch.where(relabeling.reversed.unsqueeze(1), chows.flip(1), chows)
    return torch.cat((blocks.reshape(-1, CHOW_ACTIONS), chows, mask[:, CHOW_ACTIONS + 3 :]), dim=1)


def relabel_actions(actions: Tensor, relabeling: Relabeling) -> Tensor:
    tile_action = actions < CHOW_ACTIONS
    tile = actions.remainder(TILE_KINDS).unsqueeze(1)
    moved = actions - tile.squeeze(1) + relabeling.target.gather(1, tile).squeeze(1)
    chow = (actions >= CHOW_ACTIONS) & (actions < CHOW_ACTIONS + 3)
    flipped = 2 * CHOW_ACTIONS + 2 - actions
    relabeled = torch.where(tile_action, moved, actions)
    relabeled = torch.where(chow & relabeling.reversed, flipped, relabeled)
    if bool(((relabeled < 0) | (relabeled >= ACTION_COUNT)).any()):
        raise RuntimeError("relabeling produced an out-of-range action")
    return relabeled
