# stylecop-rs

A Rust command that checks the StyleCop rules a syntax tree can decide, and prints the same SA codes.

`dotnet build` still spends a large share of its time inside Roslyn analyzers. StyleCop.Analyzers is one of the slow ones, and people still turn it on for layout, spacing, and ordering. This repo exists to make that pass fast enough that the first run feels wrong.

## Version one

- Reads the repo's `.editorconfig`.
- Prints diagnostics in StyleCop's shape: file, line, column, SA code, message.
- Covers layout, spacing, ordering, and the readability and maintainability checks a syntax walk can decide. Those rules do not need the compiler's symbol table.
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

`<path>` is a directory, a source file, or an `.editorconfig`. The command prints whether that file contains a StyleCop setting, then spacing, layout, readability, maintainability, and ordering diagnostics. Findings use the per-rule and StyleCop category severity settings for each source file. A missing input path produces an error and exits with status 2.

A StyleCop setting is a `stylecop.*` key, a `dotnet_diagnostic.SA` severity, or a `dotnet_analyzer_diagnostic.category-StyleCop.*` severity. Those are the keys in StyleCop.Analyzers' configuration doc. Shared editorconfig keys such as `indent_size` do not count.

## Using-directive ordering

The CLI covers the seven using-directive rules and the member, keyword, and accessor order rules SA1201–SA1207, SA1212–SA1214. Member order reads `settings.orderingRules.elementOrder`. The default is kind, accessibility, constant, static, then readonly. SA1412 stays off unless a severity setting enables it, matching the pinned analyzer.

| Rule | Check |
| --- | --- |
| SA1200 | Placement inside or outside namespaces |
| SA1208 | `System` directives before other namespace imports |
| SA1209 | Alias directives after namespace imports |
| SA1210 | Alphabetical namespace imports |
| SA1211 | Alphabetical alias names |
| SA1216 | Static imports between namespace imports and aliases |
| SA1217 | Alphabetical static imports |

Use `settings.orderingRules` in `stylecop.json` or `.stylecop.json` to set
`systemUsingDirectivesFirst` or `usingDirectivesPlacement`. The CLI can find
literal configuration paths in a nearby project's `AdditionalFiles` items and
settings files beside or above the source. It also reads
`dotnet_sort_system_directives_first` and `csharp_using_directive_placement`
in applicable EditorConfig sections. The pinned analyzer expects a notification
suffix on placement values such as `outside_namespace:warning`. Explicit JSON
settings take precedence.

The corpus checks use the pinned analyzer. Known compatibility gaps include
source-level `#pragma warning` suppression, some Japanese name comparisons,
and spacing checks inside raw string interpolations.

## Diagnostic parity

Install Python 3.9 or newer, Git, Rust, and [.NET SDK 10.0.115](https://github.com/dotnet/core/blob/main/release-notes/10.0/10.0.15/10.0.115.md).
The comparison fetches the pinned corpora from GitHub and restores the analyzer from NuGet.

```sh
python3 benchmarks/autofac/compare.py --corpus all --parity-only
```

The command builds both tools once, verifies a positive control on each corpus,
and reports the full diagnostic differences for Autofac and Dapper. Each corpus
runs even when the other has differences or a failed control. The command
returns 0 for parity, 1 for diagnostic differences, and 2 for setup or
control failures.
Use this command to verify changes to spacing, layout, ordering, and the syntax rules.

## Autofac parity and benchmark

```sh
python3 benchmarks/autofac/compare.py --require-speedup
```

The Autofac run checks out Autofac at `dd36d18ada9be8f5403aa2b8bd77dcbe9666ee20`
in a temporary directory and compares SA1000–SA1028, SA1106–SA1108, SA1110–SA1119, SA1120,
SA1122–SA1124, SA1127, SA1128, SA1132–SA1134, SA1136, SA1137, SA1200–SA1214, SA1216, SA1217,
SA1402, SA1403, SA1407, SA1408, SA1411–SA1413, and SA1500–SA1520 against StyleCop.Analyzers.Unstable 1.2.0.556. Both tools analyze the tracked C# files in
`src/Autofac` with Autofac's `.editorconfig` and Debug `net10.0` preprocessor
symbols. The Roslyn runner also loads Autofac's `stylecop.json` and uses warning
severity for the selected rules, matching the pinned project's ruleset.
The comparison checks repository-relative paths, line and column, rule IDs,
severity, and messages, including duplicates. A temporary source file verifies
that both tools detect SA1028, SA1517, and all seven using-directive ordering
rules before each corpus run.

The median uses five runs after a warmup, with fresh processes for both tools on
every run. Rust timing covers the release CLI from
process launch through output. Roslyn timing covers configuration loading, source
reading and parsing, and running the selected analyzers. It excludes .NET process
startup, analyzer loading, and report serialization. Checkout, restore, and tool
builds run before timing. The result measures spacing, layout, readability, maintainability, and ordering analysis on
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
