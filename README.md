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

`<path>` is a directory, a source file, or an `.editorconfig`. The command prints whether that file contains a StyleCop setting, then spacing diagnostics SA1000 through SA1028 and layout diagnostics SA1500 through SA1520.

A StyleCop setting is a `stylecop.*` key, a `dotnet_diagnostic.SA` severity, or a `dotnet_analyzer_diagnostic.category-StyleCop.*` severity. Those are the keys in StyleCop.Analyzers' configuration doc. Shared editorconfig keys such as `indent_size` do not count.

## Autofac parity and benchmark

Install Python 3, Git, Rust, and [.NET SDK 10.0.115](https://github.com/dotnet/core/blob/main/release-notes/10.0/10.0.15/10.0.115.md).
The command fetches the pinned checkout from GitHub and restores the analyzer from NuGet.

```sh
python3 benchmarks/autofac/compare.py --require-speedup
```

The command checks out Autofac at `dd36d18ada9be8f5403aa2b8bd77dcbe9666ee20`
in a temporary directory and compares SA1000–SA1028 and SA1500–SA1520 against
StyleCop.Analyzers.Unstable 1.2.0.556. Both tools analyze the tracked C# files in
`src/Autofac` with Autofac's `.editorconfig` and Debug `net10.0` preprocessor
symbols. The Roslyn runner also loads Autofac's `stylecop.json` and uses warning
severity for the selected rules, matching the pinned project's ruleset.
The comparison checks repository-relative paths, line and column, rule IDs,
severity, and messages, including duplicates. A temporary file with a known
SA1028 violation verifies that both tools can report a finding before the
corpus run.

The median uses five runs after a warmup, with fresh processes for both tools on
every run. Rust timing covers the release CLI from
process launch through output. Roslyn timing covers configuration loading, source
reading and parsing, and running the selected analyzers. It excludes .NET process
startup, analyzer loading, and report serialization. Checkout, restore, and tool
builds run before timing. The result measures spacing and layout analysis on
the source corpus. It does not measure Autofac's full build.

Use `--dotnet /path/to/dotnet` if the pinned SDK is installed under a separate
host. Use `--samples N` to set the sample count. Diagnostic differences always
fail the command. `--require-speedup` also fails the command when the median
speedup is below 10×.
