use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn expect(name: &str, lines: &[&str]) {
    let path = fixture(name);
    let text = stylecop_rs::report(&path);
    let shown = path.display().to_string();
    let mut expected = String::from("stylecop-section: absent\n");
    for line in lines {
        expected.push_str(&shown);
        expected.push_str(line);
        expected.push('\n');
    }
    assert_eq!(text, expected);
}

#[test]
fn syntax_order_and_shape_rules_match_the_pinned_analyzer() {
    expect(
        "SyntaxRules.cs",
        &[
            "(3,19): warning SA1205: Partial elements should declare an access modifier",
            "(3,19): warning SA1402: File may only contain a single type",
            "(7,20): warning SA1402: File may only contain a single type",
            "(15,9): warning SA1132: Each field should be declared on its own line",
            "(25,16): warning SA1201: A constructor should not follow a method",
            "(29,30): warning SA1128: Put constructor initializers on their own line",
            "(35,13): warning SA1212: A get accessor appears after a set accessor within a property or indexer",
            "(39,42): warning SA1201: A event should not follow a property",
            "(41,13): warning SA1213: Event accessors should follow order",
            "(45,18): warning SA1207: The keyword 'protected' should come before 'internal'",
            "(45,32): warning SA1201: A field should not follow a event",
            "(47,23): warning SA1206: The 'static' modifier should appear before 'async'",
            "(50,13): warning SA1108: Block statements should not contain embedded comments",
            "(50,13): warning SA1515: Single-line comment should be preceded by blank line",
            "(54,25): warning SA1119: Statement should not use unnecessary parenthesis",
            "(55,29): warning SA1407: Arithmetic expressions should declare precedence",
            "(56,25): warning SA1408: Conditional expressions should declare precedence",
            "(60,28): warning SA1122: Use string.Empty for empty strings",
            "(61,29): warning SA1127: Generic type constraints should be on their own line",
            "(61,45): warning SA1502: Element should not be on a single line",
            "(62,26): warning SA1107: Code should not contain multiple statements on one line",
            "(63,13): warning SA1106: Code should not contain empty statements",
            "(64,13): warning SA1123: Region should not be located within a code element",
            "(69,9): warning SA1124: Do not use regions",
            "(71,14): warning SA1201: A enum should not follow a method",
            "(73,16): warning SA1136: Enum values should be on separate lines",
            "(73,16): warning SA1413: Use trailing comma in multi-line initializers",
            "(78,9): warning SA1134: Each attribute should be placed on its own line of code",
            "(78,25): warning SA1411: Attribute constructor should not use unnecessary parenthesis",
            "(78,29): warning SA1133: Each attribute should be placed in its own set of square brackets",
            "(78,73): warning SA1201: A field should not follow a enum",
            "(81,15): warning SA1201: A namespace should not follow a class",
            "(81,15): warning SA1403: File may only contain a single namespace",
            "(85,11): warning SA1402: File may only contain a single type",
            "(91,21): warning SA1202: 'public' members should come before 'private' members",
            "(95,13): warning SA1201: A field should not follow a method",
            "(97,19): warning SA1203: Constant fields should appear before non-constant fields",
            "(101,22): warning SA1214: Readonly fields should appear before non-readonly fields",
            "(107,21): warning SA1204: Static members should appear before non-static members",
        ],
    );
}

#[test]
fn sa1412_warns_for_a_file_without_a_utf8_bom_when_enabled() {
    let dir = std::env::temp_dir().join(format!("stylecop-rs-sa1412-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(".editorconfig"),
        "root = true\n\n[*.cs]\ndotnet_diagnostic.SA1412.severity = warning\n",
    )
    .unwrap();
    let file = dir.join("Bom.cs");
    std::fs::write(&file, "namespace A;\n\ninternal class Bom\n{\n}\n").unwrap();
    let text = stylecop_rs::report(&file);
    let shown = file.display();
    assert_eq!(
        text,
        format!(
            "stylecop-section: present\n{shown}(1,1): warning SA1412: Store files as UTF-8 with byte order mark\n"
        )
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
