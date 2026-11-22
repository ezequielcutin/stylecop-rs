import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

from compare import diagnostic_diff, parse_rust, parse_oracle


class ComparisonTests(unittest.TestCase):
    def test_compares_all_fields_and_keeps_duplicates(self):
        with TemporaryDirectory() as folder:
            root = Path(folder)
            source = root / "A (old).cs"
            line = f"{source}(3,4): warning SA1028: Trailing space"
            rust = parse_rust(f"stylecop-section: absent\n{line}\n{line}\n", root)
            oracle, seconds = parse_oracle(
                '{"seconds":0.125,"diagnostics":['
                '["A (old).cs",3,4,"SA1028","warning","Trailing space"]]}'
            )
            self.assertEqual(seconds, 0.125)
            self.assertEqual(len(rust), 2)
            self.assertEqual(diagnostic_diff(rust, oracle), ([], [rust[0]]))
            self.assertEqual(diagnostic_diff(rust[:1], oracle), ([], []))
            changes = {
                0: "B.cs",
                1: 2,
                2: 5,
                3: "SA1000",
                4: "error",
                5: "Different message",
            }
            for field, value in changes.items():
                changed = list(oracle[0])
                changed[field] = value
                with self.subTest(field=field):
                    self.assertEqual(
                        diagnostic_diff(rust[:1], [tuple(changed)]),
                        ([tuple(changed)], [rust[0]]),
                    )

    def test_rejects_unrecognized_output_and_outside_paths(self):
        with TemporaryDirectory() as folder:
            root = Path(folder)
            with self.assertRaises(ValueError):
                parse_rust("unexpected output", root)
            with self.assertRaises(ValueError):
                parse_rust(f"stylecop-section: absent\n"
                           f"{root.parent / 'A.cs'}(1,2): warning SA1028: message", root)
            with self.assertRaises(ValueError):
                parse_oracle('{"seconds":0,"diagnostics":[]}')
