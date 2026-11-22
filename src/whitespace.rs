//! Whitespace spacing rules that walk trivia: SA1028 first.

use crate::config::FileSettings;
use crate::lex::{Token, TokenKind, Trivia, TriviaKind, flatten_trivia, whitespace_only};
use crate::source::{Source, is_dotnet_whitespace};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub path: String,
    pub line: u32,
    pub column: u32,
    pub code: &'static str,
    pub message: String,
}

pub fn whitespace_diagnostics(
    source: &Source,
    tokens: &[Token],
    path: &str,
    settings: &FileSettings,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if whitespace_only(tokens) {
        return diagnostics;
    }
    sa1004(source, tokens, path, &mut diagnostics);
    sa1005(source, tokens, path, &mut diagnostics);
    sa1006(source, tokens, path, &mut diagnostics);
    sa1025(source, tokens, path, &mut diagnostics);
    sa1027(source, tokens, path, settings, &mut diagnostics);
    sa1028(source, tokens, path, &mut diagnostics);
    diagnostics
}

fn sa1028(source: &Source, tokens: &[Token], path: &str, diagnostics: &mut Vec<Diagnostic>) {
    let mut flat = Vec::new();
    flatten_trivia(tokens, &mut flat);
    let mut previous: Option<&Trivia> = None;
    for trivia in &flat {
        match trivia.kind {
            TriviaKind::Whitespace | TriviaKind::EndOfLine => {}
            TriviaKind::SingleLineComment => {
                report_span_trailing(source, path, trivia.start, trivia.end, diagnostics);
            }
            TriviaKind::MultiLineComment => {
                report_multiline_comment(source, path, trivia.start, trivia.end, diagnostics);
            }
            TriviaKind::DocComment => {
                report_doc_trailing(source, path, trivia, diagnostics);
            }
            _ => {}
        }
        if trivia.kind == TriviaKind::EndOfLine
            && let Some(prev) = previous
            && prev.end == trivia.start
            && prev.kind == TriviaKind::Whitespace
        {
            report(source, path, prev.start, "SA1028", MSG_1028, diagnostics);
        }
        previous = Some(trivia);
    }
    if let Some(prev) = previous
        && prev.kind == TriviaKind::Whitespace
        && prev.end as usize == source.text.len()
    {
        report(source, path, prev.start, "SA1028", MSG_1028, diagnostics);
    }
}

const MSG_1028: &str = "Code should not contain trailing whitespace";

fn report_doc_trailing(
    source: &Source,
    path: &str,
    trivia: &Trivia,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut previous: Option<&Token> = None;
    for token in &trivia.tokens {
        if token.kind == TokenKind::XmlNewLine
            && let Some(prev) = previous
            && prev.kind == TokenKind::XmlText
            && prev.end == token.start
        {
            report_span_trailing(source, path, prev.start, prev.end, diagnostics);
        }
        previous = Some(token);
    }
}

fn report_multiline_comment(
    source: &Source,
    path: &str,
    start: u32,
    end: u32,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let first = source.line_index(start as usize);
    for index in first..source.line_count() {
        let (line_start, line_end) = source.line_content(index);
        if line_end > end as usize {
            break;
        }
        if line_end > start as usize {
            report_span_trailing(
                source,
                path,
                line_start as u32,
                line_end as u32,
                diagnostics,
            );
        }
        if line_end == source.text.len() {
            break;
        }
    }
}

fn report_span_trailing(
    source: &Source,
    path: &str,
    start: u32,
    end: u32,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(ws) = trailing_whitespace(source, start, end) else {
        return;
    };
    report(source, path, ws, "SA1028", MSG_1028, diagnostics);
}

fn trailing_whitespace(source: &Source, start: u32, end: u32) -> Option<u32> {
    if start >= end {
        return None;
    }
    let slice = source.slice(start, end);
    let mut kept = 0usize;
    let mut last_non: Option<usize> = None;
    for ch in slice.chars() {
        let next = kept + ch.len_utf8();
        if !is_dotnet_whitespace(ch) {
            last_non = Some(next);
        }
        kept = next;
    }
    let ws = start as usize + last_non.unwrap_or(0);
    if ws >= end as usize {
        None
    } else {
        Some(ws as u32)
    }
}

const MSG_1004: &str = "Documentation line should begin with a space";
const MSG_1005: &str = "Single line comment should begin with a space";
const MSG_1025: &str = "Code should not contain multiple whitespace characters in a row";
const MSG_1027: &str = "Tabs and spaces should be used correctly";

fn sa1004(source: &Source, tokens: &[Token], path: &str, diagnostics: &mut Vec<Diagnostic>) {
    let mut flat = Vec::new();
    flatten_trivia(tokens, &mut flat);
    for trivia in flat {
        if trivia.kind != TriviaKind::DocComment {
            continue;
        }
        for token in &trivia.tokens {
            if token
                .leading
                .iter()
                .any(|item| item.kind == TriviaKind::Exterior)
            {
                check_doc_line(source, path, token, diagnostics);
            }
        }
    }
}

fn check_doc_line(source: &Source, path: &str, token: &Token, diagnostics: &mut Vec<Diagnostic>) {
    let text = source.slice(token.start, token.end);
    let exterior = token
        .leading
        .iter()
        .find(|item| item.kind == TriviaKind::Exterior);
    let exterior_text = exterior
        .map(|item| source.slice(item.start, item.end))
        .unwrap_or("");
    if token.kind == TokenKind::XmlNewLine {
        return;
    }
    if token.kind == TokenKind::XmlText {
        // A blank `///` line attaches its exterior to an empty text token. StyleCop
        // treats that line as a newline and does not report SA1004.
        if text.is_empty() {
            return;
        }
        if text.starts_with("  ") {
            if token.doc_depth > 0 {
                return;
            }
        } else if text.starts_with(' ') || exterior_text.ends_with(' ') {
            return;
        }
        report(source, path, token.start, "SA1004", MSG_1004, diagnostics);
        return;
    }
    if let Some(last) = token.leading.last() {
        let last_text = source.slice(last.start, last.end);
        if last.kind == TriviaKind::Whitespace && last_text.starts_with(' ') {
            return;
        }
        if last.kind == TriviaKind::Exterior && last_text.ends_with(' ') {
            return;
        }
    }
    report(source, path, token.start, "SA1004", MSG_1004, diagnostics);
}

fn sa1005(source: &Source, tokens: &[Token], path: &str, diagnostics: &mut Vec<Diagnostic>) {
    let mut is_first = true;
    let mut newline_count = 0;
    for token in tokens {
        for trivia in token.leading.iter().chain(token.trailing.iter()) {
            match trivia.kind {
                TriviaKind::SingleLineComment => {
                    check_line_comment(source, path, trivia, is_first, diagnostics);
                    is_first = false;
                    newline_count = 0;
                }
                TriviaKind::EndOfLine => {
                    newline_count += 1;
                    if newline_count == 2 {
                        is_first = true;
                        newline_count = 0;
                    }
                }
                TriviaKind::Whitespace => {}
                _ => is_first = true,
            }
        }
    }
}

fn check_line_comment(
    source: &Source,
    path: &str,
    trivia: &Trivia,
    is_first: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let text = source.slice(trivia.start, trivia.end);
    if text == "//"
        || text.starts_with("///")
        || text.starts_with("//--")
        || text.starts_with("//\\")
    {
        return;
    }
    let mut spaces = 0;
    for ch in text.chars().skip(2) {
        if ch == ' ' {
            spaces += 1;
        } else {
            break;
        }
    }
    if spaces == 1 || (spaces > 1 && !is_first) {
        return;
    }
    report(source, path, trivia.start, "SA1005", MSG_1005, diagnostics);
}

fn sa1006(source: &Source, tokens: &[Token], path: &str, diagnostics: &mut Vec<Diagnostic>) {
    let mut flat = Vec::new();
    flatten_trivia(tokens, &mut flat);
    for trivia in flat {
        if trivia.kind != TriviaKind::Directive {
            continue;
        }
        let hash_end = trivia.start + 1;
        let spaced = trivia
            .inner
            .iter()
            .any(|item| item.kind == TriviaKind::Whitespace && item.start == hash_end);
        if !spaced {
            continue;
        }
        let keyword = source.slice(trivia.detail_start, trivia.detail_end);
        if keyword.is_empty() {
            continue;
        }
        let message = format!("Preprocessor keyword '{keyword}' should not be preceded by a space");
        report(
            source,
            path,
            trivia.detail_start,
            "SA1006",
            &message,
            diagnostics,
        );
    }
}

fn sa1025(source: &Source, tokens: &[Token], path: &str, diagnostics: &mut Vec<Diagnostic>) {
    for (index, token) in tokens.iter().enumerate() {
        let previous = if index == 0 {
            None
        } else {
            Some(&tokens[index - 1])
        };
        let next = tokens.get(index + 1);
        check_whitespace_run(
            source,
            path,
            &token.leading,
            previous,
            Some(token),
            diagnostics,
        );
        check_whitespace_run(
            source,
            path,
            &token.trailing,
            Some(token),
            next,
            diagnostics,
        );
    }
}

fn check_whitespace_run(
    source: &Source,
    path: &str,
    trivia: &[Trivia],
    preceding: Option<&Token>,
    following: Option<&Token>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (index, item) in trivia.iter().enumerate() {
        if item.kind != TriviaKind::Whitespace {
            continue;
        }
        if source.slice(item.start, item.end).chars().count() <= 1 {
            continue;
        }
        if source.line_column(item.start as usize).1 == 1 {
            continue;
        }
        if preceding.is_some_and(|token| is_comma_or_semicolon(source, token)) {
            continue;
        }
        if trivia
            .get(index + 1)
            .is_some_and(|next| next.kind == TriviaKind::EndOfLine)
        {
            continue;
        }
        if following.is_none_or(|token| token.kind == TokenKind::EndOfFile) {
            continue;
        }
        report(source, path, item.start, "SA1025", MSG_1025, diagnostics);
    }
}

fn is_comma_or_semicolon(source: &Source, token: &Token) -> bool {
    matches!(source.slice(token.start, token.end), "," | ";")
}

fn sa1027(
    source: &Source,
    tokens: &[Token],
    path: &str,
    settings: &FileSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let excluded = excluded_spans(source, tokens);
    let bytes = source.text.as_bytes();
    let mut index = 0usize;
    let mut excluded_index = 0usize;
    let mut line_start = 0usize;
    while index < bytes.len() {
        if excluded_index < excluded.len() && index == excluded[excluded_index].0 as usize {
            let end = excluded[excluded_index].1 as usize;
            if end == 0 {
                break;
            }
            index = end - 1;
            excluded_index += 1;
            index += 1;
            continue;
        }
        let ch = bytes[index];
        if ch == b'\n' || ch == b'\r' {
            index += 1;
            if ch == b'\r' && index < bytes.len() && bytes[index] == b'\n' {
                index += 1;
            }
            line_start = index;
            continue;
        }
        if ch != b' ' && ch != b'\t' {
            index += 1;
            continue;
        }
        let run_start = index;
        let mut tab_count = 0usize;
        let mut contains_spaces = false;
        let mut tab_after_space = false;
        while index < bytes.len() {
            if excluded_index < excluded.len() && index == excluded[excluded_index].0 as usize {
                break;
            }
            if bytes[index] == b' ' {
                contains_spaces = true;
                index += 1;
            } else if bytes[index] == b'\t' {
                tab_count += 1;
                tab_after_space = contains_spaces;
                index += 1;
            } else {
                break;
            }
        }
        let use_tabs = settings.use_tabs;
        let tab_size = settings.tab_size;
        if use_tabs && run_start == line_start {
            let space_count = (index - run_start) - tab_count;
            if tab_after_space || space_count >= tab_size {
                report(
                    source,
                    path,
                    run_start as u32,
                    "SA1027",
                    MSG_1027,
                    diagnostics,
                );
            }
        } else if tab_count > 0 {
            report(
                source,
                path,
                run_start as u32,
                "SA1027",
                MSG_1027,
                diagnostics,
            );
        }
    }
}

fn excluded_spans(source: &Source, tokens: &[Token]) -> Vec<(u32, u32)> {
    let mut spans = Vec::new();
    for token in tokens {
        collect_token(source, token, &mut spans);
    }
    spans.sort_unstable();
    merge_spans(spans)
}

fn collect_token(source: &Source, token: &Token, spans: &mut Vec<(u32, u32)>) {
    if matches!(
        token.kind,
        TokenKind::String | TokenKind::Char | TokenKind::InterpolatedText
    ) && token.end > token.start
    {
        spans.push((token.start, token.end));
    }
    for trivia in token.leading.iter().chain(token.trailing.iter()) {
        collect_trivia(source, trivia, spans);
    }
}

fn collect_trivia(source: &Source, trivia: &Trivia, spans: &mut Vec<(u32, u32)>) {
    if trivia.kind == TriviaKind::DisabledText && trivia.end > trivia.start {
        spans.push((trivia.start, trivia.end));
    }
    if trivia.kind == TriviaKind::SingleLineComment
        && source.slice(trivia.start, trivia.end).starts_with("////")
    {
        spans.push((trivia.start, trivia.end));
    }
    for inner in &trivia.inner {
        collect_trivia(source, inner, spans);
    }
    for token in &trivia.tokens {
        collect_token(source, token, spans);
    }
}

fn merge_spans(spans: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
    let mut merged: Vec<(u32, u32)> = Vec::new();
    for (start, end) in spans {
        if let Some(last) = merged.last_mut()
            && start <= last.1
        {
            last.1 = last.1.max(end);
            continue;
        }
        merged.push((start, end));
    }
    merged
}

fn report(
    source: &Source,
    path: &str,
    byte: u32,
    code: &'static str,
    message: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let (line, column) = source.line_column(byte as usize);
    diagnostics.push(Diagnostic {
        path: path.to_string(),
        line,
        column,
        code,
        message: message.to_string(),
    });
}
