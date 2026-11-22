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
fn readability_rules_match_the_pinned_analyzer() {
    expect(
        "Readability.cs",
        &[
            "(6,1): warning SA1137: Elements should have the same indentation",
            "(11,9): warning SA1120: Comments should contain text",
            "(12,9): warning SA1120: Comments should contain text",
            "(19,13): warning SA1110: Opening parenthesis or bracket should be on declaration line",
            "(22,30): warning SA1116: The parameters should begin on the line after the declaration, whenever the parameter span across multiple lines",
            "(24,9): warning SA1009: Closing parenthesis should not be preceded by a space",
            "(24,9): warning SA1111: Closing parenthesis should be on line of last parameter",
            "(31,13): warning SA1009: Closing parenthesis should not be preceded by a space",
            "(31,13): warning SA1112: Closing parenthesis should be on line of opening parenthesis",
            "(34,23): warning SA1116: The parameters should begin on the line after the declaration, whenever the parameter span across multiple lines",
            "(35,9): warning SA1001: Commas should not be preceded by whitespace",
            "(35,9): warning SA1113: Comma should be on the same line as previous parameter",
            "(43,13): warning SA1114: Parameter list should follow declaration",
            "(46,28): warning SA1116: The parameters should begin on the line after the declaration, whenever the parameter span across multiple lines",
            "(48,9): warning SA1115: The parameter should begin on the line after the previous parameter",
            "(54,22): warning SA1116: The parameters should begin on the line after the declaration, whenever the parameter span across multiple lines",
            "(61,13): warning SA1117: The parameters should all be placed on the same line or each parameter should be placed on its own line",
            "(66,24): warning SA1118: The parameter spans multiple lines",
        ],
    );
}
