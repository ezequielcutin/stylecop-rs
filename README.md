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

## Status

No rules yet. The first task is to measure which SA rules actually cost time, on a public repo that already references StyleCop.Analyzers. Analyzer-summary seconds are cumulative and can exceed the wall-clock build, so the writeup has to keep those separate.

Stop if that time is almost all symbol-level documentation rules. That is a typechecker, and it is the wrong first cut.

Stop later, once a syntax subset exists, if it is not at least 10× faster than StyleCop on that repo, or if the findings diverge.
