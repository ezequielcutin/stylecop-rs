/// True when `text` is an `.editorconfig` body that contains a StyleCop setting.
///
/// Recognized keys are the ones StyleCop.Analyzers documents in Configuration.md:
/// `stylecop.*`, `dotnet_diagnostic.SA<digits>`, and
/// `dotnet_analyzer_diagnostic.category-StyleCop.*`.
/// Shared editorconfig keys that StyleCop also reads, such as `indent_size`,
/// are not a StyleCop section.
pub fn stylecop_section_present(text: &str) -> bool {
    text.lines().any(line_has_stylecop_key)
}

/// One hardcoded diagnostic in the `file(line,column): warning SAxxxx: message` shape.
pub fn sample_diagnostic(path: &str) -> String {
    format!("{path}(1,1): warning SA0000: Sample diagnostic. No rules are implemented.")
}

/// Section line plus the sample diagnostic for `path`.
pub fn report(path: &std::path::Path) -> String {
    let present = editorconfig_text(path).is_some_and(|text| stylecop_section_present(&text));
    let section = if present { "present" } else { "absent" };
    format!(
        "stylecop-section: {section}\n{}",
        sample_diagnostic(&path.display().to_string())
    )
}

fn editorconfig_text(path: &std::path::Path) -> Option<String> {
    let file = editorconfig_path(path)?;
    std::fs::read_to_string(file).ok()
}

fn editorconfig_path(path: &std::path::Path) -> Option<std::path::PathBuf> {
    if path.is_file() {
        if path.file_name().and_then(|name| name.to_str()) == Some(".editorconfig") {
            return Some(path.to_path_buf());
        }
        let sibling = path.parent()?.join(".editorconfig");
        return sibling.is_file().then_some(sibling);
    }
    let direct = path.join(".editorconfig");
    direct.is_file().then_some(direct)
}

fn line_has_stylecop_key(raw: &str) -> bool {
    let line = raw.trim();
    if line.is_empty() || line.starts_with('#') || line.starts_with(';') || line.starts_with('[') {
        return false;
    }
    let Some((key, _)) = line.split_once('=') else {
        return false;
    };
    is_stylecop_key(key.trim())
}

fn is_stylecop_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.starts_with("stylecop.")
        || diagnostic_id(&key, "dotnet_diagnostic.sa")
        || key.starts_with("dotnet_analyzer_diagnostic.category-stylecop.")
}

fn diagnostic_id(key: &str, prefix: &str) -> bool {
    let Some(rest) = key.strip_prefix(prefix) else {
        return false;
    };
    rest.chars().next().is_some_and(|ch| ch.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_editorconfig_keys_are_not_a_stylecop_section() {
        let text = "[*.cs]\nindent_size = 4\nindent_style = space\ndotnet_sort_system_directives_first = true\n";
        assert!(!stylecop_section_present(text));
    }

    #[test]
    fn stylecop_ordering_key_is_a_stylecop_section() {
        let text = "[*.cs]\nstylecop.ordering.elementOrder = kind, accessibility\n";
        assert!(stylecop_section_present(text));
    }

    #[test]
    fn sa_severity_key_is_a_stylecop_section() {
        let text = "dotnet_diagnostic.SA1027.severity = none\n";
        assert!(stylecop_section_present(text));
    }

    #[test]
    fn stylecop_category_severity_is_a_stylecop_section() {
        let text =
            "dotnet_analyzer_diagnostic.category-StyleCop.CSharp.OrderingRules.severity = none\n";
        assert!(stylecop_section_present(text));
    }

    #[test]
    fn sample_diagnostic_uses_file_line_column_code_and_message() {
        let line = sample_diagnostic("src/Widget.cs");
        assert_eq!(
            line,
            "src/Widget.cs(1,1): warning SA0000: Sample diagnostic. No rules are implemented."
        );
    }

    #[test]
    fn report_reads_editorconfig_beside_a_directory() {
        let dir = std::env::temp_dir().join(format!("stylecop-rs-report-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(".editorconfig"),
            "[*.cs]\nstylecop.layout.newlineAtEndOfFile = require\n",
        )
        .unwrap();
        let text = report(&dir);
        assert!(text.starts_with("stylecop-section: present\n"), "{text}");
        assert!(text.ends_with(&sample_diagnostic(&dir.display().to_string())));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn report_without_editorconfig_still_prints_the_sample() {
        let dir = std::env::temp_dir().join(format!("stylecop-rs-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let text = report(&dir);
        assert!(text.starts_with("stylecop-section: absent\n"), "{text}");
        assert!(text.ends_with(&sample_diagnostic(&dir.display().to_string())));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
