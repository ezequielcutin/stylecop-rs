# stylecop-rs

A Rust command that checks the StyleCop rules a syntax tree can decide, and prints the same SA codes.

`dotnet build` still spends a large share of its time inside Roslyn analyzers. StyleCop.Analyzers is one of the slow ones, and people still turn it on for layout, spacing, and ordering. This repo exists to make that pass fast enough that the first run feels wrong.

## Version one

- Reads the repo's `.editorconfig`.
- Prints diagnostics in StyleCop's shape: file, line, column, SA code, message.
- Covers layout, spacing, and ordering. Those rules do not need the compiler's symbol table.
- Diffs every finding against StyleCop.Analyzers on a real repository. A mismatch is a bug.

## Out of scope

- The C# compiler, and `dotnet build` itself. Compilation stays.
- The built-in CA analyzers. Those need Roslyn's flow analysis.
- Documentation and naming rules that need a compilation. They stay out until the syntax rules match.
- Formatting. CSharpier already does that.

## Run

```
cargo run -- <path>
```

`<path>` is a directory, a source file, or an `.editorconfig`. The command prints whether that file contains a StyleCop setting, then spacing diagnostics SA1000 through SA1028 and layout diagnostics SA1500 through SA1520. Findings use the per-rule and StyleCop category severity settings for each source file. A missing input path produces an error and exits with status 2.

A StyleCop setting is a `stylecop.*` key, a `dotnet_diagnostic.SA` severity, or a `dotnet_analyzer_diagnostic.category-StyleCop.*` severity. Those are the keys in StyleCop.Analyzers' configuration doc. Shared editorconfig keys such as `indent_size` do not count.

## Spacing and layout parity

Install Python 3, Git, Rust, and [.NET SDK 10.0.115](https://github.com/dotnet/core/blob/main/release-notes/10.0/10.0.15/10.0.115.md).
The comparison fetches the pinned corpora from GitHub and restores the analyzer from NuGet.

```sh
python3 benchmarks/autofac/compare.py --corpus all --parity-only
```

The command builds both tools once, verifies a positive control on each corpus,
and reports the full diagnostic differences for Autofac and Dapper. Each corpus
runs even when the other has differences or a failed control. The command
returns 0 for parity, 1 for diagnostic differences, and 2 for setup or
control failures.
Use this command to verify changes to spacing and layout rules.

## Autofac parity and benchmark

```sh
python3 benchmarks/autofac/compare.py --require-speedup
```

The Autofac run checks out Autofac at `dd36d18ada9be8f5403aa2b8bd77dcbe9666ee20`
in a temporary directory and compares SA1000–SA1028 and SA1500–SA1520 against
StyleCop.Analyzers.Unstable 1.2.0.556. Both tools analyze the tracked C# files in
`src/Autofac` with Autofac's `.editorconfig` and Debug `net10.0` preprocessor
symbols. The Roslyn runner also loads Autofac's `stylecop.json` and uses warning
severity for the selected rules, matching the pinned project's ruleset.
The comparison checks repository-relative paths, line and column, rule IDs,
severity, and messages, including duplicates. A temporary file with known
SA1028 and SA1517 violations verifies that both tools report spacing and layout
findings before the corpus run.

The median uses five runs after a warmup, with fresh processes for both tools on
every run. Rust timing covers the release CLI from
process launch through output. Roslyn timing covers configuration loading, source
reading and parsing, and running the selected analyzers. It excludes .NET process
startup, analyzer loading, and report serialization. Checkout, restore, and tool
builds run before timing. The result measures spacing and layout analysis on
the source corpus. It does not measure Autofac's full build.

Use `--dotnet /path/to/dotnet` if the pinned SDK is installed under a separate
host. Use `--samples N` to set the sample count. Use `--corpus all --require-speedup`
to check the 10× median speed target on both corpora.

### Dapper parity check

```sh
python3 benchmarks/autofac/compare.py --corpus dapper --parity-only
```

This compares the 51 tracked C# files in `Dapper/` from Dapper 2.1.66
(`bd4f75b512de3e00f2c2631d5309961a1ecfea23`). It uses Dapper's root
`.editorconfig`, the same warning profile as the Autofac comparison, and
the Rust CLI's Debug `net10.0` preprocessor symbols for both tools. The report
lists each remaining mismatch and exits with status 1 if it finds any.
