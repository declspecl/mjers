import unittest
from dataclasses import replace

from mjers.evaluate import Comparison, WallResult, paired_difference


class PairedEvaluationTests(unittest.TestCase):
    def test_uncertainty_uses_paired_wall_differences(self) -> None:
        baseline = (3, -1, -1, -1)
        first_walls = (WallResult(1, (4, 4, 4, 4), baseline), WallResult(2, (12, 12, 12, 12), baseline))
        second_walls = (WallResult(1, (6, 6, 6, 6), baseline), WallResult(2, (14, 14, 14, 14), baseline))
        first = Comparison("rules", "teacher", (1, 2), 8.0, 8.0, 4.0, first_walls)
        second = Comparison("rules", "teacher", (1, 2), 10.0, 10.0, 4.0, second_walls)
        result = paired_difference(first, second)
        self.assertEqual(result.second_minus_first_units_per_hand, 2.0)
        self.assertEqual(result.paired_standard_error, 0.0)
        self.assertEqual(first_walls[0].paired_delta(), 4.0)
        with self.assertRaisesRegex(ValueError, "wall seeds"):
            paired_difference(first, replace(second, seeds=(3, 4)))
        with self.assertRaisesRegex(ValueError, "opponents"):
            paired_difference(first, replace(second, baseline_json="other"))
        with self.assertRaisesRegex(ValueError, "inconsistent"):
            paired_difference(
                first, replace(second, walls=(replace(second_walls[0], baseline_units_by_seat=(0, 0, 0, 0)), second_walls[1]))
            )


if __name__ == "__main__":
    unittest.main()
