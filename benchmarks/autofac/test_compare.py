from contextlib import redirect_stderr, redirect_stdout
from io import StringIO
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from types import SimpleNamespace
from unittest.mock import patch

from compare import benchmark, diagnostic_diff, parse_rust, parse_oracle


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

    def test_all_corpora_check_once_and_continue_after_a_mismatch(self):
        def checkout_stub(work, corpus):
            root = work / corpus.name
            (root / corpus.source).mkdir(parents=True)
            return root, [(corpus.source / "Example.cs").as_posix()]

        def analyze_stub(command, rust, root, target, files, manifest):
            if target.is_file():
                control = files[0]
                known = [
                    (control, 1, 1, "SA1517", "warning", "Leading blank line"),
                    (control, 6, 2, "SA1028", "warning", "Trailing space"),
                ]
                return known, known, 0.01, 0.1

            findings = [(files[0], 2, 1, "SA1517", "warning", "Leading blank line")]
            actual = [] if root.name == "Autofac" else findings
            return actual, findings, 0.01, 0.1

        args = SimpleNamespace(
            corpus="all", parity_only=True, require_speedup=False, dotnet="dotnet", samples=5
        )
        with TemporaryDirectory() as folder:
            work = Path(folder)
            with (
                patch("compare.build_tools", return_value=(work / "rust", work / "oracle")) as build,
                patch("compare.checkout", side_effect=checkout_stub),
                patch("compare.analyze", side_effect=analyze_stub) as analyze,
                redirect_stdout(StringIO()) as output,
            ):
                self.assertEqual(benchmark(args, work), 1)

        build.assert_called_once_with(work, "dotnet")
        self.assertEqual(analyze.call_count, 4)
        self.assertIn("Autofac", output.getvalue())
        self.assertIn("Dapper", output.getvalue())
        self.assertIn("1 diagnostic differences", output.getvalue())
        self.assertIn("Parity: 0 differences", output.getvalue())
        self.assertNotIn("Speedup", output.getvalue())

    def test_corpus_setup_failure_is_named_and_does_not_skip_other_corpora(self):
        args = SimpleNamespace(corpus="all", dotnet="dotnet")
        work = Path("benchmark")
        with (
            patch("compare.build_tools", return_value=(work / "rust", work / "oracle")),
            patch("compare.benchmark_corpus", side_effect=[ValueError("failed control"), 1])
            as compare_corpus,
            redirect_stderr(StringIO()) as errors,
        ):
            self.assertEqual(benchmark(args, work), 2)

        self.assertEqual(compare_corpus.call_count, 2)
        self.assertIn("Autofac comparison failed: failed control", errors.getvalue())
