//! Editorconfig severity and indentation for files under analysis.
//!
//! Shared keys such as `indent_size` are read when a rule needs them. They do
//! not make a StyleCop section by themselves.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

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

#[derive(Clone, Debug)]
pub struct FileSettings {
    by_id: HashMap<String, Level>,
    by_category: HashMap<String, Level>,
    pub use_tabs: bool,
    pub tab_size: usize,
    pub newline_at_end: NewlineAtEnd,
}

impl FileSettings {
    pub fn for_source(path: &Path) -> Self {
        let mut settings = Self {
            by_id: HashMap::new(),
            by_category: HashMap::new(),
            use_tabs: false,
            tab_size: 4,
            newline_at_end: NewlineAtEnd::Allow,
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
