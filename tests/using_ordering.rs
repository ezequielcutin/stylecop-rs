use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn ordering_report(path: &std::path::Path) -> Vec<String> {
    let report = stylecop_rs::report(path);
    let shown = path.display().to_string();
    report
        .lines()
        .filter_map(|line| line.strip_prefix(&shown))
        .filter(|line| line.contains(" SA12"))
        .map(str::to_owned)
        .collect()
}

#[test]
fn using_ordering_matches_all_seven_pinned_rules() {
    assert_eq!(
        ordering_report(&fixture("UsingOrdering.cs")),
        [
            "(2,1): warning SA1200: Using directive should appear within a namespace declaration",
            "(6,5): warning SA1210: Using directives should be ordered alphabetically by the namespaces",
            "(8,5): warning SA1208: Using directive for 'System' should appear before directive for 'Alpha'",
            "(10,5): warning SA1209: Using alias directives should be placed after all using namespace directives",
            "(10,5): warning SA1211: Using alias directive for 'Apple' should appear before using alias directive for 'Zebra'",
            "(12,5): warning SA1216: Using static directives should be placed at the correct location",
            "(12,5): warning SA1217: The using static directive for 'Zulu.Tools' should appear after the using static directive for 'Alpha.Tools'",
        ]
    );
}

#[test]
fn using_ordering_respects_placement_and_diagnostic_settings() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("stylecop-using-order-{stamp}"));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("Example.cs");
    fs::write(&path, "namespace Example;\n\nusing Z;\nusing System;\n").unwrap();
    fs::write(
        directory.join(".editorconfig"),
        "root = true\n[*.cs]\n\
         dotnet_analyzer_diagnostic.category-StyleCop.CSharp.OrderingRules.severity = none\n\
         dotnet_diagnostic.SA1200.severity = error\n",
    )
    .unwrap();
    fs::write(
        directory.join("stylecop.json"),
        r#"{"settings":{"orderingRules":{"usingDirectivesPlacement":"outsideNamespace"}}}"#,
    )
    .unwrap();

    let findings = ordering_report(&path);
    fs::remove_dir_all(&directory).unwrap();
    assert_eq!(
        findings,
        [
            "(3,1): error SA1200: Using directive should appear outside a namespace declaration",
            "(4,1): error SA1200: Using directive should appear outside a namespace declaration",
        ]
    );
}

#[test]
fn using_ordering_matches_scope_boundaries_and_invariant_name_order() {
    assert_eq!(
        ordering_report(&fixture("UsingScopes.cs")),
        [
            "(1,1): warning SA1200: Using directive should appear within a namespace declaration",
            "(1,1): warning SA1210: Using directives should be ordered alphabetically by the namespaces",
            "(2,1): warning SA1200: Using directive should appear within a namespace declaration",
            "(4,1): warning SA1200: Using directive should appear within a namespace declaration",
            "(4,1): warning SA1210: Using directives should be ordered alphabetically by the namespaces",
            "(5,1): warning SA1200: Using directive should appear within a namespace declaration",
            "(7,1): warning SA1200: Using directive should appear within a namespace declaration",
            "(12,5): warning SA1208: Using directive for 'System.Linq' should appear before directive for 'global::System'",
            "(16,9): warning SA1210: Using directives should be ordered alphabetically by the namespaces",
            "(33,5): warning SA1210: Using directives should be ordered alphabetically by the namespaces",
            "(34,5): warning SA1210: Using directives should be ordered alphabetically by the namespaces",
        ]
    );
}

#[test]
fn alias_order_uses_invariant_casing_and_identifier_values() {
    assert_eq!(
        ordering_report(&fixture("UsingAliases.cs")),
        [
            "(4,5): warning SA1211: Using alias directive for 'Z' should appear before using alias directive for 'İ'",
            "(10,5): warning SA1211: Using alias directive for 'Apple' should appear before using alias directive for 'Ze\u{301}ta'",
            "(16,5): warning SA1211: Using alias directive for 'Apple' should appear before using alias directive for 'Zeta'",
        ]
    );
}
