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
fn keyword_operator_and_sign_spacing_matches_stylecop() {
    expect(
        "Spacing.cs",
        &[
            "(3,17): warning SA1001: Commas should be followed by whitespace",
            "(5,9): warning SA1000: The keyword 'if' should be followed by a space",
            "(5,13): warning SA1003: Operator '==' should be followed by whitespace.",
            "(5,13): warning SA1003: Operator '==' should be preceded by whitespace.",
            "(5,18): warning SA1501: Statement should not be on a single line",
            "(7,17): warning SA1026: The keyword 'new' should not be followed by a space or a blank line",
            "(13,9): warning SA1000: The keyword 'default' should not be followed by a space",
            "(14,18): warning SA1003: Operator '+' should be followed by whitespace.",
            "(14,18): warning SA1003: Operator '+' should be preceded by whitespace.",
            "(15,15): warning SA1003: Operator '+' should be followed by whitespace.",
            "(16,15): warning SA1001: Commas should not be preceded by whitespace",
            "(18,11): warning SA1003: Operator '=' should be followed by whitespace.",
            "(20,14): warning SA1014: Opening generic brackets should not be preceded by a space",
            "(22,13): warning SA1019: Member access symbol '.' should not be preceded by a space",
            "(24,11): warning SA1003: Operator '++' should not be preceded by whitespace.",
            "(24,11): warning SA1020: Increment symbol '++' should not be preceded by a space",
            "(26,9): warning SA1021: Negative sign should not be followed by a space",
            "(28,13): warning SA1018: Nullable type symbol should not be preceded by a space",
        ],
    );
}

#[test]
fn brackets_braces_parentheses_and_colons_match_stylecop() {
    expect(
        "SpacingMore.cs",
        &[
            "(3,11): warning SA1002: Semicolons should not be preceded by a space",
            "(4,10): warning SA1002: Semicolons should be followed by a space",
            "(5,21): warning SA1007: Operator keyword should be followed by a space",
            "(6,1): warning SA1516: Elements should be separated by blank line",
            "(8,12): warning SA1008: Opening parenthesis should not be followed by a space.",
            "(8,17): warning SA1501: Statement should not be on a single line",
            "(9,13): warning SA1009: Closing parenthesis should not be preceded by a space",
            "(10,27): warning SA1010: Opening square brackets should not be preceded by a space",
            "(11,29): warning SA1011: Closing square bracket should not be preceded by a space",
            "(12,25): warning SA1011: Closing square bracket should be followed by a space",
            "(12,26): warning SA1012: Opening brace should be preceded by a space",
            "(13,10): warning SA1513: Closing brace should be followed by blank line",
            "(14,1): warning SA1516: Elements should be separated by blank line",
            "(14,5): warning SA1016: Opening attribute brackets should not be followed by a space",
            "(15,11): warning SA1017: Closing attribute brackets should not be preceded by a space",
            "(16,14): warning SA1502: Element should not be on a single line",
            "(17,1): warning SA1516: Elements should be separated by blank line",
            "(17,13): warning SA1015: Closing generic bracket should not be followed by a space",
            "(17,15): warning SA1019: Member access symbol '.' should not be preceded by a space",
            "(18,13): warning SA1022: Positive sign should not be followed by a space",
            "(19,1): warning SA1516: Elements should be separated by blank line",
            "(19,27): warning SA1502: Element should not be on a single line",
            "(19,37): warning SA1023: Dereference symbol '*' should not be followed by a space",
            "(20,1): warning SA1516: Elements should be separated by blank line",
            "(24,20): warning SA1024: Colon should not be preceded by a space",
        ],
    );
}

#[test]
fn a_closing_brace_jammed_against_else_is_sa1013() {
    expect(
        "CloseBrace.cs",
        &[
            "(5,19): warning SA1012: Opening brace should be followed by a space",
            "(5,19): warning SA1501: Statement should not be on a single line",
            "(5,20): warning SA1013: Closing brace should be followed by a space",
            "(5,20): warning SA1013: Closing brace should be preceded by a space",
            "(5,26): warning SA1501: Statement should not be on a single line",
        ],
    );
}

#[test]
fn sa1010_allows_space_before_collection_expressions() {
    let path = fixture("CollectionExpressions.cs");
    let report = stylecop_rs::report(&path);
    let findings: Vec<_> = report
        .lines()
        .filter(|line| line.contains("warning SA1010:"))
        .collect();
    assert_eq!(
        findings,
        [format!(
            "{}(8,16): warning SA1010: Opening square brackets should not be preceded by a space",
            path.display()
        )]
    );
}

#[test]
fn sa1009_matches_stylecop_for_wrapped_closing_parentheses_and_conditional_colons() {
    let path = fixture("ClosingParentheses.cs");
    let report = stylecop_rs::report(&path);
    let shown = path.display().to_string();
    let findings: Vec<_> = report
        .lines()
        .filter(|line| line.contains("warning SA1009:"))
        .map(|line| line.strip_prefix(&shown).unwrap())
        .collect();
    assert_eq!(
        findings,
        [
            "(9,13): warning SA1009: Closing parenthesis should not be preceded by a space",
            "(15,9): warning SA1009: Closing parenthesis should not be preceded by a space",
            "(16,40): warning SA1009: Closing parenthesis should be followed by a space",
            "(17,56): warning SA1009: Closing parenthesis should be followed by a space",
            "(17,60): warning SA1009: Closing parenthesis should be followed by a space",
            "(23,9): warning SA1009: Closing parenthesis should not be preceded by a space",
            "(28,9): warning SA1009: Closing parenthesis should not be preceded by a space",
            "(48,9): warning SA1009: Closing parenthesis should not be preceded by a space",
        ]
    );
}

#[test]
fn conditional_colons_require_spaces_on_both_sides() {
    expect(
        "ConditionalColons.cs",
        &[
            "(4,30): warning SA1009: Closing parenthesis should be followed by a space",
            "(4,31): warning SA1003: Operator ':' should be preceded by whitespace.",
            "(4,31): warning SA1024: Colon should be preceded by a space",
            "(7,30): warning SA1003: Operator ':' should be followed by whitespace.",
            "(7,30): warning SA1024: Colon should be followed by a space",
            "(14,46): warning SA1003: Operator ':' should be preceded by whitespace.",
            "(14,46): warning SA1024: Colon should be preceded by a space",
            "(20,33): warning SA1003: Operator ':' should be preceded by whitespace.",
            "(20,33): warning SA1024: Colon should be preceded by a space",
            "(23,34): warning SA1003: Operator ':' should be followed by whitespace.",
            "(23,34): warning SA1024: Colon should be followed by a space",
            "(29,45): warning SA1010: Opening square brackets should not be preceded by a space",
            "(33,26): warning SA1003: Operator ':' should be preceded by whitespace.",
            "(33,26): warning SA1024: Colon should be preceded by a space",
        ],
    );
}

#[test]
fn interpolated_text_punctuation_does_not_count_as_code() {
    expect(
        "InterpolationSpacing.cs",
        &["(9,43): warning SA1018: Nullable type symbol should not be preceded by a space"],
    );
}

#[test]
fn return_and_case_keywords_allow_prefix_operators() {
    expect(
        "PrefixOperators.cs",
        &[
            "(19,18): warning SA1021: Negative sign should not be followed by a space",
            "(22,30): warning SA1003: Operator '++' should not be preceded by whitespace.",
            "(22,30): warning SA1020: Increment symbol '++' should not be preceded by a space",
        ],
    );
}
