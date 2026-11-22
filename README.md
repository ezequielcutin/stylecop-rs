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

`<path>` is a directory, a source file, or an `.editorconfig`. The command prints whether that file contains a StyleCop setting, then spacing diagnostics SA1000 through SA1028.

A StyleCop setting is a `stylecop.*` key, a `dotnet_diagnostic.SA` severity, or a `dotnet_analyzer_diagnostic.category-StyleCop.*` severity. Those are the keys in StyleCop.Analyzers' configuration doc. Shared editorconfig keys such as `indent_size` do not count.

## Status

The first measurement is on a public Autofac build. Syntax rules were most of StyleCop's own time. Documentation was not almost all of it, so a syntax port is still the first cut. On that build, Sonar and the built-in analyzers were the long part of the wall clock, so a StyleCop-only speedup will not make a build like that feel fast.

No rules are implemented. Stop once a syntax subset exists if it is not at least 10× faster than StyleCop on a real repo, or if the findings diverge.
