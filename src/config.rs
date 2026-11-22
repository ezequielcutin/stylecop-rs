//! Editorconfig and StyleCop settings for files under analysis.
//!
//! Shared keys such as `indent_size` are read when a rule needs them. They do
//! not make a StyleCop section by themselves.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use jsonc_parser::{JsonValue, ParseOptions, parse_to_value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Level {
    Default,
    None,
    Silent,
    Suggestion,
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// How SA1518 treats a newline at the end of a file.
///
/// `insert_final_newline` in `.editorconfig` selects require or omit. Without
/// that key StyleCop allows a missing newline and a single trailing newline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewlineAtEnd {
    Allow,
    Require,
    Omit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsingPlacement {
    Inside,
    Outside,
    Preserve,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrderingSettings {
    pub system_first: bool,
    pub placement: UsingPlacement,
}

impl Default for OrderingSettings {
    fn default() -> Self {
        Self {
            system_first: true,
            placement: UsingPlacement::Inside,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FileSettings {
    by_id: HashMap<String, Level>,
    by_category: HashMap<String, Level>,
    pub use_tabs: bool,
    pub tab_size: usize,
    pub newline_at_end: NewlineAtEnd,
    pub ordering: OrderingSettings,
}

impl FileSettings {
    pub fn for_source(path: &Path) -> Self {
        let mut settings = Self {
            by_id: HashMap::new(),
            by_category: HashMap::new(),
            use_tabs: false,
            tab_size: 4,
            newline_at_end: NewlineAtEnd::Allow,
            ordering: OrderingSettings::default(),
        };
        let mut source = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        if source.components().any(|part| part == Component::ParentDir)
            && let Ok(resolved) = source.canonicalize()
        {
            source = resolved;
        }
        for (directory, text) in editorconfig_chain(&source) {
            apply_editorconfig(&text, &directory, &source, &mut settings);
        }
        if let Some(config) = stylecop_config(&source) {
            settings.ordering = std::fs::read_to_string(config)
                .ok()
                .and_then(|text| ordering_from_json(&text, settings.ordering))
                .unwrap_or_default();
        }
        settings
    }

    pub fn severity(&self, code: &str, category: &str) -> Option<Severity> {
        let mut level = Level::Warning;
        if let Some(category_level) = self.by_category.get(category) {
            level = apply_level(*category_level, level);
        }
        if let Some(id_level) = self.by_id.get(code) {
            level = apply_level(*id_level, level);
        }
        match level {
            Level::None | Level::Silent => None,
            Level::Suggestion => Some(Severity::Info),
            Level::Warning | Level::Default => Some(Severity::Warning),
            Level::Error => Some(Severity::Error),
        }
    }
}

fn stylecop_config(source: &Path) -> Option<PathBuf> {
    let mut fallback = None;
    let mut found_project = false;
    for directory in source.ancestors().skip(1) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        let mut configs = Vec::new();
        let mut projects = Vec::new();
        let mut repository_root = false;
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if fallback.is_none() && is_stylecop_name(name) && entry.path().is_file() {
                configs.push(entry.path());
            } else if !found_project && name.ends_with(".csproj") {
                projects.push(entry.path());
            } else if name == ".git" {
                repository_root = true;
            }
        }
        if fallback.is_none() {
            configs.sort_unstable();
            // Prefer the conventional spelling if both basenames exist.
            fallback = configs
                .iter()
                .find(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.eq_ignore_ascii_case("stylecop.json"))
                })
                .cloned()
                .or_else(|| configs.into_iter().next());
        }
        if !projects.is_empty() {
            // The nearest directory with projects owns the source. In a shared
            // project directory, try project paths in a deterministic order.
            found_project = true;
            projects.sort_unstable();
            if let Some(config) = projects
                .iter()
                .find_map(|path| project_stylecop_config(path))
            {
                return Some(config);
            }
        }
        if repository_root || (found_project && fallback.is_some()) {
            break;
        }
    }
    fallback
}

fn is_stylecop_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("stylecop.json") || name.eq_ignore_ascii_case(".stylecop.json")
}

fn project_stylecop_config(project: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(project).ok()?;
    let document = roxmltree::Document::parse(&text).ok()?;
    let root = document.root_element();
    root.children()
        .filter(|node| node.tag_name().name() == "ItemGroup")
        .flat_map(|group| group.children())
        .filter(|node| node.tag_name().name() == "AdditionalFiles")
        .filter_map(|node| node.attribute("Include"))
        .flat_map(|includes| includes.split(';'))
        // Resolve literal paths relative to the project, accepting MSBuild's
        // Windows-style separators on Unix.
        .map(|include| project.with_file_name(include.trim().replace('\\', "/")))
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(is_stylecop_name)
                && path.is_file()
        })
}

fn ordering_from_json(text: &str, mut settings: OrderingSettings) -> Option<OrderingSettings> {
    // LightJson, used by StyleCop, allows comments and trailing commas.
    let options = ParseOptions {
        allow_comments: true,
        allow_trailing_commas: true,
        allow_loose_object_property_names: false,
        allow_missing_commas: false,
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
        allow_bare_decimal_point_numbers: false,
        allow_non_finite_numbers: false,
        allow_extended_string_escapes: false,
    };
    let JsonValue::Object(root) =
        parse_to_value(text.trim_start_matches('\u{feff}'), &options).ok()??
    else {
        return None;
    };
    let object = root.get_object("settings")?;
    let Some(ordering) = object.get("orderingRules") else {
        return Some(settings);
    };
    let JsonValue::Object(ordering) = ordering else {
        return None;
    };
    if let Some(value) = ordering.get("systemUsingDirectivesFirst") {
        let JsonValue::Boolean(system_first) = value else {
            return None;
        };
        settings.system_first = *system_first;
    }
    if let Some(value) = ordering.get("usingDirectivesPlacement") {
        let JsonValue::String(placement) = value else {
            return None;
        };
        settings.placement = match placement.trim().to_ascii_lowercase().as_str() {
            "insidenamespace" => UsingPlacement::Inside,
            "outsidenamespace" => UsingPlacement::Outside,
            "preserve" => UsingPlacement::Preserve,
            _ => return None,
        };
    }
    Some(settings)
}

fn apply_level(update: Level, current: Level) -> Level {
    if update == Level::Default {
        current
    } else {
        update
    }
}

fn editorconfig_chain(source: &Path) -> Vec<(PathBuf, String)> {
    let mut found = Vec::new();
    for directory in source.ancestors().skip(1) {
        if let Ok(text) = std::fs::read_to_string(directory.join(".editorconfig")) {
            let rooted = text_is_root(&text);
            found.push((directory.to_path_buf(), text));
            if rooted {
                break;
            }
        }
    }
    found.reverse();
    found
}

fn text_is_root(text: &str) -> bool {
    text.lines()
        .map(str::trim)
        .take_while(|line| !line.starts_with('['))
        .any(|line| {
            let Some((key, value)) = line.split_once('=') else {
                return false;
            };
            key.trim().eq_ignore_ascii_case("root") && value.trim().eq_ignore_ascii_case("true")
        })
}

fn apply_editorconfig(text: &str, directory: &Path, source: &Path, settings: &mut FileSettings) {
    let mut active = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            active = section_matches(&line[1..line.len() - 1], directory, source);
            continue;
        }
        if !active {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        if key == "indent_style" {
            settings.use_tabs = value.eq_ignore_ascii_case("tab");
        } else if key == "insert_final_newline" {
            if value.eq_ignore_ascii_case("true") {
                settings.newline_at_end = NewlineAtEnd::Require;
            } else if value.eq_ignore_ascii_case("false") {
                settings.newline_at_end = NewlineAtEnd::Omit;
            }
        } else if key == "tab_width" {
            if let Ok(size) = value.parse::<usize>() {
                settings.tab_size = size.max(1);
            }
        } else if key == "dotnet_sort_system_directives_first" {
            settings.ordering.system_first = !value.eq_ignore_ascii_case("false");
        } else if key == "csharp_using_directive_placement" {
            // The pinned analyzer reads the option only with a notification suffix.
            settings.ordering.placement = match value.split_once(':') {
                Some(("outside_namespace", _)) => UsingPlacement::Outside,
                _ => UsingPlacement::Inside,
            };
        } else if let Some(id) = diagnostic_id(&key) {
            settings.by_id.insert(id, parse_level(value));
        } else if let Some(category) = category_name(&key) {
            settings.by_category.insert(category, parse_level(value));
        }
    }
}

fn diagnostic_id(key: &str) -> Option<String> {
    let rest = key
        .strip_prefix("dotnet_diagnostic.")?
        .strip_suffix(".severity")?;
    let (prefix, digits) = rest.split_at_checked(2)?;
    if prefix == "sa" && !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit()) {
        Some(rest.to_ascii_uppercase())
    } else {
        None
    }
}

fn category_name(key: &str) -> Option<String> {
    Some(
        key.strip_prefix("dotnet_analyzer_diagnostic.category-")?
            .strip_suffix(".severity")?
            .to_string(),
    )
}

fn parse_level(value: &str) -> Level {
    match value.to_ascii_lowercase().as_str() {
        "none" => Level::None,
        "silent" => Level::Silent,
        "suggestion" => Level::Suggestion,
        "warning" => Level::Warning,
        "error" => Level::Error,
        "default" => Level::Default,
        _ => Level::Default,
    }
}

fn section_matches(header: &str, directory: &Path, source: &Path) -> bool {
    let patterns = expand_patterns(header);
    if !header.contains('/') {
        let name = source
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        return patterns
            .iter()
            .any(|pattern| glob_bytes(pattern.as_bytes(), name.as_bytes()));
    }
    let Ok(relative) = source.strip_prefix(directory) else {
        return false;
    };
    let Some(components) = relative
        .iter()
        .map(|part| part.to_str())
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    let relative = components.join("/");
    patterns
        .iter()
        .any(|pattern| glob_path(pattern.trim_start_matches('/'), &relative))
}

fn glob_path(pattern: &str, path: &str) -> bool {
    let path: Vec<_> = path.chars().collect();
    let mut pattern = pattern.chars().peekable();
    // Each entry tracks whether the pattern so far matches that prefix of the path.
    let mut matches = vec![false; path.len() + 1];
    matches[0] = true;
    while let Some(part) = pattern.next() {
        match part {
            '*' if pattern.next_if_eq(&'*').is_some() => {
                for index in 1..matches.len() {
                    matches[index] |= matches[index - 1];
                }
            }
            '*' => {
                for (index, character) in path.iter().enumerate() {
                    matches[index + 1] |= *character != '/' && matches[index];
                }
            }
            literal => {
                for (index, character) in path.iter().enumerate().rev() {
                    matches[index + 1] = matches[index]
                        && (*character == literal || (literal == '?' && *character != '/'));
                }
                matches[0] = false;
            }
        }
    }
    matches[path.len()]
}

fn expand_patterns(header: &str) -> Vec<String> {
    if let Some(start) = header.find('{')
        && let Some(rel_end) = header[start + 1..].find('}')
    {
        let end = start + 1 + rel_end;
        let prefix = &header[..start];
        let suffix = &header[end + 1..];
        return header[start + 1..end]
            .split(',')
            .flat_map(|alt| expand_patterns(&format!("{prefix}{alt}{suffix}")))
            .collect();
    }
    if header.contains(',') {
        return header
            .split(',')
            .map(|part| part.trim().to_string())
            .collect();
    }
    vec![header.trim().to_string()]
}

fn glob_bytes(pattern: &[u8], text: &[u8]) -> bool {
    let mut pattern_index = 0;
    let mut text_index = 0;
    let mut star: Option<usize> = None;
    let mut star_text = 0;
    while text_index < text.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'?' || pattern[pattern_index] == text[text_index])
        {
            pattern_index += 1;
            text_index += 1;
            continue;
        }
        if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star = Some(pattern_index);
            star_text = text_index;
            pattern_index += 1;
            continue;
        }
        if let Some(star_at) = star {
            star_text += 1;
            text_index = star_text;
            pattern_index = star_at + 1;
            continue;
        }
        return false;
    }
    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let directory = std::env::temp_dir()
                .join(format!("stylecop-config-{}-{stamp}", std::process::id()));
            fs::create_dir(&directory).unwrap();
            let scratch = Self(directory);
            scratch.write(".git", "");
            scratch.write(".editorconfig", "root = true\n");
            scratch
        }

        fn write(&self, path: &str, contents: &str) {
            let path = self.0.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }

        fn ordering(&self, source: &str) -> OrderingSettings {
            FileSettings::for_source(&self.0.join(source)).ordering
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn ordering_uses_pinned_defaults() {
        let repo = Scratch::new();
        let settings = repo.ordering("Example.cs");
        assert!(settings.system_first);
        assert_eq!(settings.placement, UsingPlacement::Inside);
    }

    #[test]
    fn ordering_reads_matching_editorconfig_and_resets_unset_options() {
        let repo = Scratch::new();
        repo.write(
            ".editorconfig",
            "root = true\n[*.cs]\ndotnet_sort_system_directives_first = FALSE\n\
             csharp_using_directive_placement = outside_namespace:suggestion\n\
             [Reset.cs]\ndotnet_sort_system_directives_first = unset\n\
             csharp_using_directive_placement = outside_namespace\n",
        );

        let settings = repo.ordering("Example.cs");
        assert!(!settings.system_first);
        assert_eq!(settings.placement, UsingPlacement::Outside);

        let settings = repo.ordering("Reset.cs");
        assert!(settings.system_first);
        assert_eq!(settings.placement, UsingPlacement::Inside);
    }

    #[test]
    fn json_overrides_only_the_ordering_options_it_supplies() {
        let repo = Scratch::new();
        repo.write(
            ".editorconfig",
            "root = true\n[*.cs]\ndotnet_sort_system_directives_first = false\n\
             csharp_using_directive_placement = outside_namespace:error\n",
        );
        repo.write(
            "stylecop.json",
            r#"{
                // StyleCop allows comments and trailing commas.
                "settings": { "orderingRules": { "usingDirectivesPlacement": "Preserve", }, },
            }"#,
        );
        let settings = repo.ordering("src/Example.cs");
        assert!(!settings.system_first);
        assert_eq!(settings.placement, UsingPlacement::Preserve);
    }

    #[test]
    fn json_system_sorting_overrides_editorconfig() {
        let repo = Scratch::new();
        repo.write(
            ".editorconfig",
            "root = true\n[*.cs]\ndotnet_sort_system_directives_first = true\n\
             csharp_using_directive_placement = outside_namespace:error\n",
        );
        repo.write(
            "stylecop.json",
            r#"{"settings":{"orderingRules":{"systemUsingDirectivesFirst":false}}}"#,
        );
        let settings = repo.ordering("src/Example.cs");
        assert!(!settings.system_first);
        assert_eq!(settings.placement, UsingPlacement::Outside);
    }

    #[test]
    fn the_nearest_project_selects_its_additional_settings_file() {
        let repo = Scratch::new();
        repo.write("stylecop.json", r#"{"settings":{}}"#);
        repo.write(
            "build/.stylecop.json",
            r#"{"settings":{"orderingRules":{"usingDirectivesPlacement":"outsideNamespace"}}}"#,
        );
        repo.write(
            "src/Outer/Outer.csproj",
            r#"<Project><ItemGroup>
                <!-- <AdditionalFiles Include="../../stylecop.json" /> -->
                <AdditionalFiles Include='..\..\build\.stylecop.json' Link='stylecop.json' />
            </ItemGroup></Project>"#,
        );
        repo.write("src/Outer/Inner/Inner.csproj", "<Project />");

        assert_eq!(
            repo.ordering("src/Outer/Features/Example.cs").placement,
            UsingPlacement::Outside
        );
        assert_eq!(
            repo.ordering("src/Outer/Inner/Example.cs").placement,
            UsingPlacement::Inside
        );
    }

    #[test]
    fn invalid_ordering_values_use_analyzer_defaults() {
        let repo = Scratch::new();
        repo.write(
            ".editorconfig",
            "root = true\n[*.cs]\ndotnet_sort_system_directives_first = false\n",
        );
        repo.write(
            "stylecop.json",
            r#"{"settings":{"orderingRules":{"usingDirectivesPlacement":"invalid"}}}"#,
        );
        assert!(repo.ordering("Example.cs").system_first);
    }
}
