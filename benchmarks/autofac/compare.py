#!/usr/bin/env python3
"""Compare the release CLI with StyleCop.Analyzers on a pinned Autofac checkout."""

import argparse
from collections import Counter
import json
import math
import os
from pathlib import Path
import re
from statistics import median
import subprocess
import sys
from tempfile import TemporaryDirectory
from time import perf_counter

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
AUTOFAC = "https://github.com/autofac/Autofac.git"
REVISION = "dd36d18ada9be8f5403aa2b8bd77dcbe9666ee20"
SOURCE = Path("src/Autofac")
DIAGNOSTIC = re.compile(r"(.+)\(([1-9]\d*),([1-9]\d*)\): (\w+) (SA\d{4}): (.*)")
SPEED_TARGET = 10


def parse_rust(output, root):
    lines = output.splitlines()
    if not lines or lines[0] not in {"stylecop-section: present", "stylecop-section: absent"}:
        raise ValueError("Missing stylecop-rs report header")
    diagnostics = []
    for line in lines[1:]:
        match = DIAGNOSTIC.fullmatch(line)
        if not match:
            raise ValueError(f"Unexpected stylecop-rs output: {line!r}")
        path, row, column, severity, code, message = match.groups()
        relative = Path(path).relative_to(root).as_posix()
        diagnostics.append((relative, int(row), int(column), code, severity, message))
    return diagnostics


def parse_oracle(output):
    report = json.loads(output)
    seconds = report["seconds"]
    if not math.isfinite(seconds) or seconds <= 0:
        raise ValueError("Invalid Roslyn timing")
    return [tuple(item) for item in report["diagnostics"]], seconds


def diagnostic_diff(rust, oracle):
    rust_counts, oracle_counts = Counter(rust), Counter(oracle)
    return sorted((oracle_counts - rust_counts).elements()), sorted(
        (rust_counts - oracle_counts).elements()
    )


def run(command, cwd, *, capture=False, env=None):
    return subprocess.run(
        command, cwd=cwd, check=True, encoding="utf-8", stdout=subprocess.PIPE if capture else None,
        env=env,
    ).stdout


def build_tools(work, dotnet):
    sdk = json.loads((HERE / "global.json").read_text())["sdk"]["version"]
    actual = run([dotnet, "--version"], HERE, capture=True).strip()
    if actual != sdk:
        raise ValueError(f"Expected .NET SDK {sdk}, found {actual}")

    cargo_target = work / "rust"
    run(["cargo", "build", "--release", "--locked", "--target-dir", cargo_target], REPO)
    artifacts = work / "dotnet"
    run([dotnet, "build", "Oracle.csproj", "--configuration", "Release",
         "--artifacts-path", artifacts], HERE)
    executable = "stylecop-rs.exe" if os.name == "nt" else "stylecop-rs"
    return cargo_target / "release" / executable, artifacts / "bin/Oracle/release/Oracle.dll"


def checkout(work):
    root = work / "autofac"
    root.mkdir()
    run(["git", "init", "--quiet"], root)
    git_config = ["git", "-c", "core.autocrlf=false"]
    run([*git_config, "fetch", "--quiet", "--depth=1", AUTOFAC, REVISION], root)
    run([*git_config, "checkout", "--quiet", "--detach", "FETCH_HEAD"], root)
    if run(["git", "rev-parse", "HEAD"], root, capture=True).strip() != REVISION:
        raise ValueError("Autofac checkout did not match the pinned revision")

    tracked = set(run(
        ["git", "ls-files", "-z", "--", SOURCE / "*.cs"], root, capture=True
    ).strip("\0").split("\0"))
    files = {file.relative_to(root).as_posix() for file in (root / SOURCE).rglob("*.cs")}
    if not files or tracked != files:
        raise ValueError("Autofac source directory does not match its tracked C# file list")
    return root, sorted(files)


def analyze(command, rust, root, target, files, manifest):
    manifest.write_text("".join(f"{file}\n" for file in files), encoding="utf-8")
    oracle, oracle_seconds = parse_oracle(run([*command, root, manifest], root, capture=True))
    start = perf_counter()
    output = run([rust, target], root, capture=True)
    rust_seconds = perf_counter() - start
    return parse_rust(output, root), oracle, rust_seconds, oracle_seconds


def report_diff(rust, oracle):
    missing, extra = diagnostic_diff(rust, oracle)
    for label, diagnostics in (("Missing from Rust", missing), ("Only in Rust", extra)):
        for file, row, column, code, severity, message in diagnostics:
            print(f"{label}: {file}({row},{column}): {severity} {code}: {message}")
    return len(missing) + len(extra)


def benchmark(args, work):
    rust, oracle = build_tools(work, args.dotnet)
    root, files = checkout(work)
    command = [args.dotnet, oracle]
    manifest = work / "sources.txt"

    # Exercise the actual CLI and oracle before accepting an empty corpus result.
    control = SOURCE / "StyleCopBenchmarkControl.cs"
    control_path = root / control
    with control_path.open("xb") as source:
        source.write(b"namespace Benchmark;\n\ninternal class Control\n{\n} \n")
    try:
        actual, expected, _, _ = analyze(
            command, rust, root, control_path, [control.as_posix()], manifest
        )
        if not any(item[:4] == (control.as_posix(), 5, 2, "SA1028") for item in expected):
            raise ValueError("Roslyn did not report the known SA1028 control violation")
        if report_diff(actual, expected):
            raise ValueError("The positive control produced different diagnostics")
    finally:
        control_path.unlink()

    print(f"Positive control passed. Autofac {REVISION[:12]}, {len(files)} tracked C# files.")
    rust_times = []
    oracle_times = []
    for sample in range(args.samples + 1):
        actual, expected, rust_time, oracle_time = analyze(
            command, rust, root, root / SOURCE, files, manifest
        )
        differences = report_diff(actual, expected)
        if differences:
            print(f"{differences} diagnostic differences.")
            return 1
        if sample:
            rust_times.append(rust_time)
            oracle_times.append(oracle_time)

    rust_time, oracle_time = median(rust_times), median(oracle_times)
    speedup = oracle_time / rust_time
    print(f"Parity: 0 differences ({len(expected)} findings on each side).")
    print(f"Median of {args.samples} runs after one warmup:")
    print(f"  Rust release CLI: {rust_time * 1000:.2f} ms")
    print(f"  Roslyn + StyleCop analysis: {oracle_time * 1000:.2f} ms")
    print(f"  Speedup: {speedup:.2f}x (target: {SPEED_TARGET}x)")
    return int(args.require_speedup and speedup < SPEED_TARGET)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dotnet", default="dotnet", help="path to the .NET host")
    parser.add_argument("--samples", default=5, type=int, help="number of measured runs")
    parser.add_argument("--require-speedup", action="store_true",
                        help=f"fail if the median speedup is below {SPEED_TARGET}x")
    args = parser.parse_args()
    if args.samples < 1:
        parser.error("--samples must be at least 1")
    args.dotnet = str(Path(args.dotnet).resolve()) if Path(args.dotnet).exists() else args.dotnet
    try:
        with TemporaryDirectory(prefix="stylecop-benchmark-") as folder:
            return benchmark(args, Path(folder).resolve())
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"Benchmark failed: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
