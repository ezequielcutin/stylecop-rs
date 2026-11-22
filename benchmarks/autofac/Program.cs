using System.Collections.Immutable;
using System.Diagnostics;
using System.Globalization;
using System.Reflection;
using System.Text.Json;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.Diagnostics;
using Microsoft.CodeAnalysis.Text;

CultureInfo.CurrentCulture = CultureInfo.InvariantCulture;
CultureInfo.CurrentUICulture = CultureInfo.InvariantCulture;

var root = args[0];
var sourcePaths = File.ReadAllLines(args[1]).Select(path => Path.Combine(root, path)).ToArray();
int[] usingRules = [1200, 1208, 1209, 1210, 1211, 1216, 1217];
int[] readabilityRules = [1110, 1111, 1112, 1113, 1114, 1115, 1116, 1117, 1118, 1120, 1137];
var rules = Enumerable.Range(1000, 29).Concat(usingRules).Concat(readabilityRules).Concat(Enumerable.Range(1500, 21))
    .Select(number => $"SA{number}").ToImmutableHashSet(StringComparer.Ordinal);

var styleCopPath = Path.Combine(AppContext.BaseDirectory, "StyleCop.Analyzers.dll");
var analyzers = Assembly.LoadFrom(styleCopPath).GetTypes()
    .Where(type => !type.IsAbstract && typeof(DiagnosticAnalyzer).IsAssignableFrom(type))
    .Select(type => (DiagnosticAnalyzer)Activator.CreateInstance(type)!)
    .Where(analyzer => analyzer.SupportedDiagnostics.Any(rule => rules.Contains(rule.Id)))
    .ToImmutableArray();
var supported = analyzers.SelectMany(analyzer => analyzer.SupportedDiagnostics).Select(rule => rule.Id);
var unavailable = rules.Except(supported).Order().ToArray();
if (unavailable.Length > 0)
{
    throw new InvalidOperationException($"StyleCop could not load rules: {string.Join(", ", unavailable)}");
}

// Use the same Debug net10.0 symbols as the Rust lexer for both corpora.
string[] symbols = [
    "TRACE", "DEBUG", "NET", "NET10_0", "NETCOREAPP",
    "NET5_0_OR_GREATER", "NET6_0_OR_GREATER", "NET7_0_OR_GREATER", "NET8_0_OR_GREATER",
    "NET9_0_OR_GREATER", "NET10_0_OR_GREATER",
    "NETCOREAPP1_0_OR_GREATER", "NETCOREAPP1_1_OR_GREATER",
    "NETCOREAPP2_0_OR_GREATER", "NETCOREAPP2_1_OR_GREATER", "NETCOREAPP2_2_OR_GREATER",
    "NETCOREAPP3_0_OR_GREATER", "NETCOREAPP3_1_OR_GREATER",
];
var parseOptions = new CSharpParseOptions(
    LanguageVersion.CSharp14, DocumentationMode.Diagnose, preprocessorSymbols: symbols);

var timer = Stopwatch.StartNew();
var configPath = Path.Combine(root, ".editorconfig");
var config = AnalyzerConfig.Parse(File.ReadAllText(configPath), configPath);
var configs = AnalyzerConfigSet.Create(ImmutableArray.Create(config));
var styleCopConfig = Path.Combine(root, "build", "stylecop.json");
ImmutableArray<AdditionalText> additional = File.Exists(styleCopConfig) ? [new DiskText(styleCopConfig)] : [];
var options = new AnalyzerOptions(additional, new ConfigOptionsProvider(configs));

var trees = sourcePaths.Select(path =>
{
    using var source = File.OpenRead(path);
    return CSharpSyntaxTree.ParseText(SourceText.From(source), parseOptions, path);
});
var levels = analyzers.SelectMany(analyzer => analyzer.SupportedDiagnostics)
    .Select(rule => rule.Id).Distinct()
    .ToImmutableDictionary(id => id, id => rules.Contains(id) ? ReportDiagnostic.Warn : ReportDiagnostic.Suppress);
var compilation = CSharpCompilation.Create("StyleCopComparison", trees, options:
    new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary,
        specificDiagnosticOptions: levels, nullableContextOptions: NullableContextOptions.Enable));
var diagnostics = await compilation.WithAnalyzers(analyzers, options).GetAnalyzerDiagnosticsAsync();
timer.Stop();

var failures = diagnostics.Where(diagnostic => diagnostic.Id is "AD0001" or "AD0002").ToArray();
if (failures.Length > 0)
{
    throw new InvalidOperationException(string.Join(Environment.NewLine, failures.AsEnumerable()));
}

var findings = diagnostics.Where(diagnostic => rules.Contains(diagnostic.Id) && !diagnostic.IsSuppressed)
    .Select(diagnostic =>
    {
        var span = diagnostic.Location.GetLineSpan();
        if (!span.IsValid)
        {
            throw new InvalidOperationException($"Diagnostic has no source location: {diagnostic}");
        }

        return new object[] {
            Path.GetRelativePath(root, span.Path).Replace(Path.DirectorySeparatorChar, '/'),
            span.StartLinePosition.Line + 1,
            span.StartLinePosition.Character + 1,
            diagnostic.Id,
            diagnostic.Severity.ToString().ToLowerInvariant(),
            diagnostic.GetMessage(CultureInfo.InvariantCulture),
        };
    });
Console.WriteLine(JsonSerializer.Serialize(new { seconds = timer.Elapsed.TotalSeconds, diagnostics = findings }));

sealed class DiskText(string path) : AdditionalText
{
    public override string Path => path;

    public override SourceText GetText(CancellationToken cancellationToken = default)
        => SourceText.From(File.ReadAllText(path));
}

sealed class ConfigOptionsProvider(AnalyzerConfigSet configs) : AnalyzerConfigOptionsProvider
{
    public override AnalyzerConfigOptions GlobalOptions => new ConfigOptions(configs.GlobalConfigOptions.AnalyzerOptions);

    public override AnalyzerConfigOptions GetOptions(SyntaxTree tree) => ForPath(tree.FilePath);

    public override AnalyzerConfigOptions GetOptions(AdditionalText textFile) => ForPath(textFile.Path);

    private ConfigOptions ForPath(string path) => new(configs.GetOptionsForSourcePath(path).AnalyzerOptions);

    private sealed class ConfigOptions(ImmutableDictionary<string, string> values) : AnalyzerConfigOptions
    {
        public override bool TryGetValue(string key, out string value) => values.TryGetValue(key, out value!);
    }
}
