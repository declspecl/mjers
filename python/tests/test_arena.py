import unittest

import numpy as np
from numpy.typing import NDArray

from mjers._native import TILE_PLANES, Arena, Env

WINS = (107, 108)


def choice(mask: NDArray[np.bool_]) -> int:
    """A deterministic stand-in student: the last legal action."""
    return int(np.flatnonzero(mask)[-1])


def play_with_env(seed: int, seat: int) -> tuple[list[NDArray[np.float32]], int]:
    env = Env("efficiency", rules="family")
    observation = env.reset(seed, seed % 4)
    seen: list[NDArray[np.float32]] = []
    while observation is not None:
        if observation.seat == seat:
            legal = np.flatnonzero(observation.action_mask)
            wins = [win for win in WINS if observation.action_mask[win]]
            if wins:
                action = wins[0]
            elif len(legal) == 1:
                action = int(legal[0])
            else:
                seen.append(observation.tiles)
                action = choice(observation.action_mask)
        else:
            action = env.teacher_action()
        transition = env.step(action)
        observation = transition.observation
        if transition.terminated:
            return seen, transition.rewards[seat]
    raise RuntimeError("hand ended without a settlement")


class ArenaTests(unittest.TestCase):
    def test_matches_env_play_for_the_same_student_choices(self) -> None:
        arena = Arena("efficiency", rules="family", slots=3)
        cases = [(seed, seed * 7 % 4) for seed in range(12)]
        expected = {case: play_with_env(*case) for case in cases}
        seen: dict[int, list[NDArray[np.float32]]] = {}
        playing: dict[int, tuple[int, int]] = {}
        results: dict[tuple[int, int], int] = {}
        queue = list(cases)

        def refill() -> None:
            while done := arena.take_finished():
                for slot, seed, seat, units in done:
                    self.assertEqual(playing.pop(slot), (seed, seat))
                    results[(seed, seat)] = units
                    np.testing.assert_array_equal(
                        np.asarray(seen.pop(slot)).reshape(-1, TILE_PLANES, 34),
                        np.asarray(expected[(seed, seat)][0]).reshape(-1, TILE_PLANES, 34),
                    )
                    if queue:
                        start(slot)

        def start(slot: int) -> None:
            seed, seat = queue.pop(0)
            playing[slot] = (seed, seat)
            seen[slot] = []
            arena.start(slot, seed, seat)

        for slot in range(3):
            start(slot)
        refill()
        while playing:
            slots, tiles, _context, mask = arena.pending()
            self.assertEqual(sorted(slots), sorted(playing))
            for row, slot in enumerate(slots):
                seen[slot].append(tiles[row])
            arena.act(slots, [choice(row) for row in mask])
            refill()
        self.assertEqual(results, {case: expected[case][1] for case in cases})

    def test_rejects_busy_slots_and_illegal_actions(self) -> None:
        arena = Arena("efficiency", rules="family", slots=1)
        arena.start(0, 3, 1)
        if not arena.take_finished():
            with self.assertRaisesRegex(ValueError, "still playing"):
                arena.start(0, 4, 1)
            slots, _tiles, _context, mask = arena.pending()
            illegal = int(np.flatnonzero(~mask[0])[0])
            with self.assertRaisesRegex(ValueError, "not legal"):
                arena.act(slots, [illegal])
        with self.assertRaisesRegex(ValueError, "no such slot"):
            arena.start(5, 0, 0)


if __name__ == "__main__":
    unittest.main()
