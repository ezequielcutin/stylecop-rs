use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn scratch_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("stylecop-rs-{name}-{nanos}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn trailing_whitespace_reports_sa1028_without_the_sample_diagnostic() {
    let dir = scratch_dir("sa1028");
    let file = dir.join("Sample.cs");
    fs::write(&file, "class C {\n    int x;  \n}\n").unwrap();

    let text = stylecop_rs::report(&file);
    let path = file.display().to_string();
    assert_eq!(
        text,
        format!(
            "stylecop-section: absent\n{path}(2,11): warning SA1028: Code should not contain trailing whitespace\n"
        )
    );

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn comment_documentation_and_preprocessor_spacing_match_stylecop() {
    let dir = scratch_dir("trivia");
    let file = dir.join("Trivia.cs");
    let source = "class C\n{\n    int x;  \n    /// <summary>\n    ///text\n    ///  nested\n    /// </summary>\n    void M() { } //comment\n    //  follow\n    ////\tint\n    string s = \"a\tb\";\n    string t = $\"a\tb {1}\";\n#if false\n\tvoid Hidden() { }\n#endif\n# if true\n#endif\n}\n";
    fs::write(&file, source).unwrap();
    let text = stylecop_rs::report(&file);
    let path = file.display().to_string();
    let expected = format!(
        "stylecop-section: absent\n\
{path}(3,11): warning SA1028: Code should not contain trailing whitespace\n\
{path}(5,8): warning SA1004: Documentation line should begin with a space\n\
{path}(8,18): warning SA1005: Single line comment should begin with a space\n\
{path}(16,3): warning SA1006: Preprocessor keyword 'if' should not be preceded by a space\n"
    );
    assert_eq!(text, expected);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_tab_outside_strings_comments_and_disabled_text_is_sa1027() {
    let dir = scratch_dir("sa1027");
    let file = dir.join("Tabs.cs");
    fs::write(
        &file,
        "class C\n{\n\tvoid Tabbed() { }\n    string s = \"a\tb\";\n    ////\tint x;\n#if false\n\thidden();\n#endif\n    void Ok() { }\n}\n",
    )
    .unwrap();
    let text = stylecop_rs::report(&file);
    let path = file.display().to_string();
    assert_eq!(
        text,
        format!(
            "stylecop-section: absent\n{path}(3,1): warning SA1027: Tabs and spaces should be used correctly\n"
        )
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn repeated_spaces_are_sa1025_except_after_comma_semicolon_or_at_end_of_line() {
    let dir = scratch_dir("sa1025");
    let file = dir.join("Multi.cs");
    fs::write(
        &file,
        "class C\n{\n    int x  = 1;\n    void M(int a,  int b) { int c;  }\n}\n",
    )
    .unwrap();
    let text = stylecop_rs::report(&file);
    let path = file.display().to_string();
    assert_eq!(
        text,
        format!(
            "stylecop-section: absent\n{path}(3,10): warning SA1025: Code should not contain multiple whitespace characters in a row\n"
        )
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn severity_none_emits_nothing_for_that_code() {
    let dir = scratch_dir("silent");
    fs::write(
        dir.join(".editorconfig"),
        "[*.cs]\ndotnet_diagnostic.SA1028.severity = none\n",
    )
    .unwrap();
    let file = dir.join("Sample.cs");
    fs::write(&file, "class C {\n    int x;  \n}\n").unwrap();
    let text = stylecop_rs::report(&file);
    assert_eq!(text, "stylecop-section: present\n");
    fs::remove_dir_all(&dir).unwrap();
}
