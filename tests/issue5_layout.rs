use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn scratch_file(name: &str, source: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("stylecop-rs-{name}-{nanos}"));
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("Sample.cs");
    fs::write(&file, source).unwrap();
    file
}

fn diagnostics(source: &str) -> String {
    let file = scratch_file("layout", source);
    let text = stylecop_rs::report(&file);
    let path = file.display().to_string();
    text.lines()
        .filter(|line| line.starts_with(&path))
        .map(|line| line.trim_start_matches(&path).to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn single_line_fields_and_interpolated_strings_are_not_separate_elements() {
    let source = r#"namespace N;

public class C
{
    private readonly object _a = new();
    private readonly (int A, int B) _b = new();
    private object _c;

    public string Description => $"Decorator ({_a})";

    public bool Equals(object other)
    {
        return false;
    }

    public override int GetHashCode()
    {
        return 1;
    }
}
"#;
    assert_eq!(diagnostics(source), "");
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn layout_rules_match_stylecop() {
    let path = fixture("Layout.cs");
    let text = stylecop_rs::report(&path);
    let shown = path.display().to_string();
    let lines = [
        "(1,1): warning SA1517: Code should not contain blank lines at start of file",
        "(9,1): warning SA1516: Elements should be separated by blank line",
        "(12,1): warning SA1506: Element documentation headers should not be followed by blank line",
        "(15,6): warning SA1513: Closing brace should be followed by blank line",
        "(16,1): warning SA1516: Elements should be separated by blank line",
        "(18,21): warning SA1500: Braces for multi-line statements should not share line",
        "(22,19): warning SA1501: Statement should not be on a single line",
        "(25,13): warning SA1503: Braces should not be omitted",
        "(28,13): warning SA1519: Braces should not be omitted from multi-line child statement",
        "(36,13): warning SA1520: Use braces consistently",
        "(43,9): warning SA1510: 'else' statement should not be preceded by a blank line",
        "(53,9): warning SA1510: 'catch' statement should not be preceded by a blank line",
        "(58,9): warning SA1510: 'finally' statement should not be preceded by a blank line",
        "(68,9): warning SA1511: While-do footer should not be preceded by blank line",
        "(71,1): warning SA1507: Code should not contain multiple blank lines in a row",
        "(74,9): warning SA1512: Single-line comments should not be followed by blank line",
        "(74,9): warning SA1515: Single-line comment should be preceded by blank line",
        "(77,9): warning SA1515: Single-line comment should be preceded by blank line",
        "(83,9): warning SA1504: All accessors should be single-line or multi-line",
        "(84,1): warning SA1516: Elements should be separated by blank line",
        "(92,5): warning SA1509: Opening braces should not be preceded by blank line",
        "(95,5): warning SA1508: A closing brace should not be preceded by a blank line",
        "(100,6): warning SA1513: Closing brace should be followed by blank line",
        "(101,1): warning SA1516: Elements should be separated by blank line",
        "(103,6): warning SA1513: Closing brace should be followed by blank line",
        "(104,5): warning SA1514: Element documentation header should be preceded by blank line",
        "(109,17): warning SA1502: Element should not be on a single line",
        "(112,5): warning SA1505: An opening brace should not be followed by a blank line",
        "(117,1): warning SA1507: Code should not contain multiple blank lines in a row",
        "(124,2): warning SA1518: Code should not contain blank lines at the end of the file",
    ];
    let mut expected = String::from("stylecop-section: absent\n");
    for line in lines {
        expected.push_str(&shown);
        expected.push_str(line);
        expected.push('\n');
    }
    assert_eq!(text, expected);
}

fn configured(name: &str, editorconfig: &str, source: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("stylecop-rs-{name}-{nanos}"));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(".editorconfig"), editorconfig).unwrap();
    let file = dir.join("Sample.cs");
    fs::write(&file, source).unwrap();
    let text = stylecop_rs::report(&file);
    let path = file.display().to_string();
    let body = text
        .lines()
        .filter(|line| line.starts_with(&path))
        .map(|line| line.trim_start_matches(&path).to_string())
        .collect::<Vec<_>>()
        .join("\n");
    fs::remove_dir_all(&dir).unwrap();
    body
}

#[test]
fn a_missing_newline_is_required_when_insert_final_newline_is_true() {
    let body = configured(
        "require-nl",
        "root = true\n\n[*]\ninsert_final_newline = true\n",
        "class C\n{\n    void M()\n    {\n    }\n}",
    );
    assert_eq!(
        body,
        "(6,2): warning SA1518: File is required to end with a single newline character"
    );
}

#[test]
fn a_trailing_newline_is_forbidden_when_insert_final_newline_is_false() {
    let body = configured(
        "omit-nl",
        "root = true\n\n[*]\ninsert_final_newline = false\n",
        "class C\n{\n    void M()\n    {\n    }\n}\n",
    );
    assert_eq!(
        body,
        "(6,2): warning SA1518: File may not end with a newline character"
    );
}

#[test]
fn layout_category_severity_none_emits_nothing() {
    let body = configured(
        "layout-silent",
        "root = true\n\n[*.cs]\ndotnet_analyzer_diagnostic.category-StyleCop.CSharp.LayoutRules.severity = none\n",
        "class C\n{\n    void A()\n    {\n    }\n    void B()\n    {\n    }\n}\n",
    );
    assert_eq!(body, "");
}

#[test]
fn one_line_catch_blocks_are_statements() {
    let path = fixture("CatchBlocks.cs");
    let report = stylecop_rs::report(&path);
    let shown = path.display().to_string();
    let findings: Vec<_> = report
        .lines()
        .filter(|line| line.contains("warning SA1501:") || line.contains("warning SA1502:"))
        .map(|line| line.strip_prefix(&shown).unwrap())
        .collect();
    assert_eq!(
        findings,
        [
            "(11,15): warning SA1501: Statement should not be on a single line",
            "(20,15): warning SA1501: Statement should not be on a single line",
            "(29,56): warning SA1501: Statement should not be on a single line",
            "(38,43): warning SA1501: Statement should not be on a single line",
            "(59,28): warning SA1501: Statement should not be on a single line",
            "(67,27): warning SA1502: Element should not be on a single line",
            "(75,63): warning SA1501: Statement should not be on a single line",
        ]
    );
}
