import unittest
from dataclasses import dataclass, replace
from pathlib import Path

import msgspec
import numpy as np

from mjers._native import ACTION_COUNT, CONTEXT_SIZE, TILE_PLANES, Env, Observation
from mjers.collect import collect


@dataclass(frozen=True)
class Payouts:
    discard_win: int
    discard_win_discarder: int
    self_draw_win: int
    per_bonus: int
    exposed_kong: int
    concealed_kong: int
    kongs_paid_without_winner: bool


@dataclass(frozen=True)
class Ruleset:
    name: str
    flower_wall_stacks: int
    allow_rob_kong: bool
    payouts: Payouts


class EnvironmentTests(unittest.TestCase):
    def assert_same_observation(self, a: Observation | None, b: Observation | None) -> None:
        if a is None or b is None:
            self.assertIs(a, b)
            return
        self.assertEqual(a.seat, b.seat)
        np.testing.assert_array_equal(a.tiles, b.tiles)
        np.testing.assert_array_equal(a.context, b.context)
        np.testing.assert_array_equal(a.action_mask, b.action_mask)

    def test_deterministic_full_hands_and_teacher_reset(self) -> None:
        for teacher in ("efficiency", "random"):
            a, b = Env(teacher, rules="family"), Env(teacher, rules="family")
            for seed in range(12):
                x, y = a.reset(seed, seed % 4), b.reset(seed, seed % 4)
                steps = 0
                while x is not None:
                    self.assert_same_observation(x, y)
                    self.assertEqual(x.tiles.shape, (TILE_PLANES, 34))
                    self.assertEqual(x.context.shape, (CONTEXT_SIZE,))
                    self.assertEqual(x.action_mask.shape, (ACTION_COUNT,))
                    self.assertEqual(x.tiles.dtype, np.float32)
                    self.assertEqual(x.context.dtype, np.float32)
                    self.assertEqual(x.action_mask.dtype, np.bool_)
                    action = a.teacher_action()
                    self.assertEqual(action, b.teacher_action())
                    self.assertTrue(x.action_mask[action])
                    tx, ty = a.step(action), b.step(action)
                    self.assertEqual(tx.rewards, ty.rewards)
                    self.assertEqual(tx.terminated, ty.terminated)
                    self.assertEqual(sum(tx.rewards), 0)
                    if not tx.terminated:
                        self.assertEqual(tx.rewards, [0] * 4)
                    self.assertEqual(tx.terminated, tx.observation is None)
                    x, y = tx.observation, ty.observation
                    steps += 1
                    self.assertLess(steps, 1000)
                self.assertIsNone(y)
                with self.assertRaises(RuntimeError):
                    a.step(0)
                with self.assertRaises(RuntimeError):
                    a.teacher_action()
            self.assert_same_observation(a.reset(0, 0), b.reset(0, 0))
            self.assertEqual(a.teacher_action(), b.teacher_action())

    def test_illegal_action_does_not_change_state(self) -> None:
        env = Env("efficiency", rules="family")
        before = env.reset(4, 0)
        assert before is not None
        illegal = int(np.flatnonzero(~before.action_mask)[0])
        for action in (illegal, ACTION_COUNT, 2**32):
            with self.assertRaises(ValueError):
                env.step(action)
            self.assert_same_observation(before, env.observe())
        with self.assertRaises(ValueError):
            env.reset(4, dealer=4)
        self.assert_same_observation(before, env.observe())

    def test_arrays_are_independent_snapshots(self) -> None:
        env = Env("efficiency", rules="family")
        before = env.reset(3, 0)
        assert before is not None
        before.tiles.fill(-1)
        before.context.fill(-1)
        before.action_mask.fill(False)
        fresh = env.observe()
        assert fresh is not None
        self.assertTrue((fresh.tiles >= 0).all())
        self.assertTrue((fresh.context >= 0).all())
        self.assertTrue(fresh.action_mask.any())
        held = fresh.tiles.copy()
        env.step(env.teacher_action())
        np.testing.assert_array_equal(held, fresh.tiles)

    def test_teacher_json_and_collection_align_outcomes_with_acting_seat(self) -> None:
        teacher = (Path(__file__).resolve().parents[2] / "strategies/tuned-family.json").read_text()
        shard = collect(Env(teacher, rules="family"), range(20, 24))
        n = len(shard.actions)
        self.assertGreater(n, 4)
        self.assertTrue(shard.action_mask[np.arange(n), shard.actions].all())
        env = Env(teacher, rules="family")
        i = 0
        for seed in range(20, 24):
            observation = env.reset(seed, seed % 4)
            start = i
            rewards: list[int] = []
            while observation is not None:
                self.assertEqual(shard.seeds[i], seed)
                self.assertEqual(shard.seats[i], observation.seat)
                np.testing.assert_array_equal(shard.tiles[i], observation.tiles)
                self.assertEqual(shard.actions[i], env.teacher_action())
                transition = env.step(int(shard.actions[i]))
                observation = transition.observation
                rewards = transition.rewards
                i += 1
            expected = np.asarray(rewards, dtype=np.int32)[shard.seats[start:i]]
            np.testing.assert_array_equal(shard.returns[start:i], expected)
        self.assertEqual(i, n)

    def test_rules_preset_and_custom_json_survive_reset(self) -> None:
        preset = Env("efficiency", rules="discarder-pays")
        rules = msgspec.json.decode(preset.rules_json, type=Ruleset)
        self.assertEqual(rules.payouts.discard_win, 0)
        custom = replace(rules, name="custom", flower_wall_stacks=9, allow_rob_kong=True)
        env = Env("efficiency", rules=msgspec.json.encode(custom).decode())
        for seed in (2, 11):
            observation = env.reset(seed, 0)
            self.assertEqual(msgspec.json.decode(env.rules_json, type=Ruleset), custom)
            rewards: list[int] = []
            while observation is not None:
                action = env.teacher_action()
                self.assertTrue(observation.action_mask[action])
                transition = env.step(action)
                observation = transition.observation
                rewards = transition.rewards
            self.assertEqual(len(rewards), 4)
            self.assertEqual(sum(rewards), 0)
        with self.assertRaises(ValueError):
            Env("efficiency", rules=msgspec.json.encode(replace(custom, flower_wall_stacks=255)).decode())
        with self.assertRaises(ValueError):
            Env("efficiency", rules="unknown")

    def test_configured_payouts_determine_collected_returns(self) -> None:
        rules = msgspec.json.decode(Env("efficiency", rules="family").rules_json, type=Ruleset)
        p = rules.payouts
        payouts = Payouts(
            3 * p.discard_win,
            3 * p.discard_win_discarder,
            3 * p.self_draw_win,
            3 * p.per_bonus,
            3 * p.exposed_kong,
            3 * p.concealed_kong,
            p.kongs_paid_without_winner,
        )
        scaled = replace(rules, name="triple-payouts", payouts=payouts)
        original = collect(Env("efficiency", rules="family"), range(20, 26))
        triple = collect(Env("efficiency", rules=msgspec.json.encode(scaled).decode()), range(20, 26))
        self.assertEqual(msgspec.json.decode(original.metadata.rules_json, type=Ruleset), rules)
        self.assertEqual(msgspec.json.decode(triple.metadata.rules_json, type=Ruleset), scaled)
        self.assertTrue((original.returns != 0).any())
        np.testing.assert_array_equal(original.tiles, triple.tiles)
        np.testing.assert_array_equal(original.actions, triple.actions)
        np.testing.assert_array_equal(triple.returns, 3 * original.returns)


if __name__ == "__main__":
    unittest.main()
