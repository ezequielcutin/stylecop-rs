use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("stylecop-cli-{}-{stamp}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        Self(dir)
    }

    fn write(&self, path: &str, contents: &str) {
        let file = self.0.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, contents).unwrap();
    }

    fn run(&self, path: &str, working_directory: &Path) -> Output {
        Command::new(env!("CARGO_BIN_EXE_stylecop-rs"))
            .current_dir(self.0.join(working_directory))
            .arg(path)
            .output()
            .unwrap()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const WITH_TRAILING_SPACE: &str = "class C\n{\n} \n";

#[test]
fn path_globs_are_relative_to_the_config_for_relative_cli_inputs() {
    let repo = Scratch::new();
    repo.write(
        ".editorconfig",
        "root = true\n[*.cs]\ndotnet_diagnostic.SA1028.severity = warning\n\
         [src/*.cs]\ndotnet_diagnostic.SA1028.severity = none\n",
    );
    repo.write(
        "src/.editorconfig",
        "[nested/**/*.cs]\ndotnet_diagnostic.SA1028.severity = error\n",
    );
    repo.write("src/Direct.cs", WITH_TRAILING_SPACE);
    repo.write("src/nested/Child.cs", WITH_TRAILING_SPACE);
    repo.write("src/nested/deep/Deep.cs", WITH_TRAILING_SPACE);
    repo.write("src/other/Elsewhere.cs", WITH_TRAILING_SPACE);
    let output = repo.run(".", Path::new("src"));
    assert!(output.status.success());
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(!report.contains("Direct.cs"), "{report}");
    assert!(report.contains("Child.cs(3,2): warning SA1028"), "{report}");
    assert!(report.contains("Deep.cs(3,2): error SA1028"), "{report}");
    assert!(
        report.contains("Elsewhere.cs(3,2): warning SA1028"),
        "{report}"
    );
}

#[test]
fn severity_flows_into_spacing_and_layout_output() {
    let repo = Scratch::new();
    repo.write(
        ".editorconfig",
        "root = true\n[*.cs]\n\
         dotnet_analyzer_diagnostic.category-StyleCop.CSharp.LayoutRules.severity = error\n\
         dotnet_analyzer_diagnostic.category-StyleCop.CSharp.SpacingRules.severity = none\n\
         dotnet_diagnostic.SA1028.severity = suggestion\n",
    );
    repo.write("Sample.cs", "\nclass C\n{\n} \n");
    let output = repo.run("Sample.cs", Path::new(""));
    assert!(output.status.success());
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(report.contains("Sample.cs(1,1): error SA1517"), "{report}");
    assert!(report.contains("Sample.cs(4,2): info SA1028"), "{report}");
}

#[test]
fn nonexistent_input_returns_an_error() {
    let repo = Scratch::new();
    let output = repo.run("missing.cs", Path::new(""));
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("missing.cs")
    );
}

#[test]
fn path_globs_anchor_brace_alternatives_and_allow_embedded_recursive_wildcards() {
    let repo = Scratch::new();
    repo.write(
        ".editorconfig",
        "root = true\n[{src/Foo.cs,Bar.cs}]\ndotnet_diagnostic.SA1028.severity = none\n\
         [src/**Target.cs]\ndotnet_diagnostic.SA1028.severity = error\n",
    );
    repo.write("Bar.cs", WITH_TRAILING_SPACE);
    repo.write("other/Bar.cs", WITH_TRAILING_SPACE);
    repo.write("src/Foo.cs", WITH_TRAILING_SPACE);
    repo.write("src/deep/Target.cs", WITH_TRAILING_SPACE);

    let output = repo.run(".", Path::new(""));
    assert!(output.status.success());
    let report = String::from_utf8(output.stdout).unwrap();
    let findings: Vec<_> = report.lines().skip(1).collect();
    assert_eq!(findings.len(), 2, "{report}");
    let nested_bar = Path::new("other").join("Bar.cs");
    assert!(
        report.contains(&format!("{}(3,2): warning SA1028", nested_bar.display())),
        "{report}"
    );
    assert!(report.contains("Target.cs(3,2): error SA1028"), "{report}");
}
