//! Readability rules a line walk can decide: SA1137, SA1120, and SA1110–SA1118.
//!
//! Indentation compares sibling statements and switch labels. Parameter rules
//! follow the pinned analyzer's line checks for calls, declarations, and lambdas.

use std::collections::HashSet;

use crate::lex::{Token, TokenKind, TriviaKind};
use crate::source::{Source, is_dotnet_whitespace};
use crate::whitespace::Diagnostic;

const SA1137: &str = "Elements should have the same indentation";
const SA1120: &str = "Comments should contain text";

pub fn readability_diagnostics(source: &Source, tokens: &[Token], path: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if tokens.is_empty() {
        return diagnostics;
    }
    let holes = hole_braces(source, tokens);
    empty_comments(source, tokens, path, &mut diagnostics);
    indentation(source, tokens, path, &holes, &mut diagnostics);
    parameter_lists(source, tokens, path, &holes, &mut diagnostics);
    diagnostics
}

fn empty_comments(
    source: &Source,
    tokens: &[Token],
    path: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for token in tokens {
        comment_list(source, path, &token.leading, diagnostics);
        comment_list(source, path, &token.trailing, diagnostics);
    }
}

fn comment_list(
    source: &Source,
    path: &str,
    trivia: &[crate::lex::Trivia],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut run = Vec::new();
    let mut blank_lines = 0;
    for item in trivia {
        match item.kind {
            TriviaKind::SingleLineComment => {
                if blank_lines >= 2 {
                    finish_comment_run(source, path, &mut run, diagnostics);
                }
                run.push(item.start);
                blank_lines = 0;
            }
            TriviaKind::EndOfLine => blank_lines += 1,
            TriviaKind::Whitespace => {}
            TriviaKind::MultiLineComment => {
                finish_comment_run(source, path, &mut run, diagnostics);
                blank_lines = 0;
                let text = source.slice(item.start, item.end);
                let inner = text
                    .strip_prefix("/*")
                    .and_then(|text| text.strip_suffix("*/"))
                    .unwrap_or("");
                if inner.chars().all(is_dotnet_whitespace) {
                    report(source, path, item.start, "SA1120", SA1120, diagnostics);
                }
            }
            _ => {
                finish_comment_run(source, path, &mut run, diagnostics);
                blank_lines = 0;
            }
        }
    }
    finish_comment_run(source, path, &mut run, diagnostics);
}

fn finish_comment_run(
    source: &Source,
    path: &str,
    run: &mut Vec<u32>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let check: Vec<u32> = if run.len() <= 1 {
        run.clone()
    } else {
        vec![run[0], *run.last().unwrap()]
    };
    for start in check {
        let token_end = source.text[start as usize..]
            .find(['\n', '\r'])
            .map(|offset| start + offset as u32)
            .unwrap_or(source.text.len() as u32);
        let text = source.slice(start, token_end);
        let body = text.strip_prefix("//").unwrap_or(text);
        if body.chars().all(is_dotnet_whitespace) {
            report(source, path, start, "SA1120", SA1120, diagnostics);
        }
    }
    run.clear();
}

struct Block {
    switch: bool,
    expect: bool,
    saw_label: bool,
    statements: Vec<(u32, String)>,
    labels: Vec<(u32, String)>,
}

fn indentation(
    source: &Source,
    tokens: &[Token],
    path: &str,
    holes: &HashSet<usize>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut blocks: Vec<Block> = Vec::new();
    let mut paren = 0i32;
    let mut bracket = 0i32;
    for index in 0..tokens.len() {
        if !is_code(tokens, index) || holes.contains(&index) {
            continue;
        }
        let text = text_at(source, tokens, index);
        match text {
            "(" => paren += 1,
            ")" => paren -= 1,
            "[" => bracket += 1,
            "]" => bracket -= 1,
            "{" if paren == 0 => {
                if let Some(block) = blocks.last_mut() {
                    block.expect = false;
                }
                blocks.push(Block {
                    switch: opens_switch(source, tokens, index),
                    expect: true,
                    saw_label: false,
                    statements: Vec::new(),
                    labels: Vec::new(),
                });
                continue;
            }
            "}" if paren == 0 => {
                if let Some(block) = blocks.pop() {
                    finish_group(source, path, &block.statements, diagnostics);
                    finish_group(source, path, &block.labels, diagnostics);
                }
                if let Some(block) = blocks.last_mut() {
                    block.expect = true;
                }
                continue;
            }
            ";" if paren == 0 && bracket == 0 => {
                if let Some(block) = blocks.last_mut() {
                    block.expect = true;
                }
                continue;
            }
            ":" if paren == 0 && bracket == 0 => {
                if let Some(block) = blocks.last_mut()
                    && block.saw_label
                {
                    block.saw_label = false;
                    block.expect = true;
                }
                continue;
            }
            _ => {}
        }
        let Some(block) = blocks.last_mut() else {
            continue;
        };
        if !block.expect || paren != 0 || bracket != 0 {
            continue;
        }
        if matches!(text, "else" | "catch" | "finally" | "while") {
            block.expect = false;
            continue;
        }
        let label = (block.switch && (text == "case" || text == "default"))
            || (tokens[index].kind == TokenKind::Identifier
                && next_code(source, tokens, holes, index)
                    .is_some_and(|next| text_at(source, tokens, next) == ":"));
        if let Some((at, indent)) = line_indent(source, &tokens[index]) {
            if label {
                block.labels.push((at, indent));
                block.saw_label = true;
            } else {
                block.statements.push((at, indent));
            }
        } else if label {
            block.saw_label = true;
        }
        block.expect = false;
    }
}

fn opens_switch(source: &Source, tokens: &[Token], brace: usize) -> bool {
    let Some(prev) = prev_code(source, tokens, &HashSet::new(), brace) else {
        return false;
    };
    if text_at(source, tokens, prev) != ")" {
        return false;
    }
    let mut depth = 0i32;
    for index in (0..prev).rev() {
        if !is_code(tokens, index) {
            continue;
        }
        match text_at(source, tokens, index) {
            ")" => depth += 1,
            "(" => {
                if depth == 0 {
                    return prev_code(source, tokens, &HashSet::new(), index)
                        .is_some_and(|before| text_at(source, tokens, before) == "switch");
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    false
}

fn finish_group(
    source: &Source,
    path: &str,
    group: &[(u32, String)],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some((_, expected)) = group.first() else {
        return;
    };
    for (at, indent) in group.iter().skip(1) {
        if indent != expected {
            report(source, path, *at, "SA1137", SA1137, diagnostics);
        }
    }
}

fn line_indent(source: &Source, token: &Token) -> Option<(u32, String)> {
    let line = source.line_index(token.start as usize);
    let (line_start, _) = source.line_content(line);
    let prefix = &source.text[line_start..token.start as usize];
    if !prefix.chars().all(is_dotnet_whitespace) {
        return None;
    }
    let at = if prefix.is_empty() {
        token.start
    } else {
        line_start as u32
    };
    Some((at, prefix.to_string()))
}

struct Piece {
    start: usize,
    end: usize,
}

fn parameter_lists(
    source: &Source,
    tokens: &[Token],
    path: &str,
    holes: &HashSet<usize>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let generics = crate::spacing::generic_indexes(source, tokens);
    let mut index = 0;
    while index < tokens.len() {
        if !is_code(tokens, index) || holes.contains(&index) {
            index += 1;
            continue;
        }
        let text = text_at(source, tokens, index);
        if (text == "(" || text == "[")
            && opens_list(source, tokens, holes, index)
            && let Some(close) = matching_close(source, tokens, holes, index)
        {
            check_list(
                source,
                tokens,
                &Lists {
                    holes,
                    generics: &generics,
                },
                path,
                index,
                close,
                diagnostics,
            );
            index = close + 1;
            continue;
        }
        index += 1;
    }
}

fn opens_list(source: &Source, tokens: &[Token], holes: &HashSet<usize>, index: usize) -> bool {
    let open = text_at(source, tokens, index);
    let Some(prev) = prev_code(source, tokens, holes, index) else {
        return open == "(" && closes_with_lambda(source, tokens, holes, index);
    };
    let prev_text = text_at(source, tokens, prev);
    if open == "(" {
        if prev_text == ">" || prev_text == "new" || prev_text == "this" || prev_text == "base" {
            return true;
        }
        if tokens[prev].kind == TokenKind::Identifier && !is_control_keyword(prev_text) {
            return true;
        }
        if prev_text == "delegate" {
            return true;
        }
        return closes_with_lambda(source, tokens, holes, index);
    }
    if tokens[prev].kind == TokenKind::Identifier && !is_control_keyword(prev_text) {
        return true;
    }
    if matches!(prev_text, "this" | "base") {
        return true;
    }
    // `items[0][1]` is element access. A following attribute list starts the next line.
    prev_text == "]" && line_of(source, tokens[prev].start) == line_of(source, tokens[index].start)
}

fn closes_with_lambda(
    source: &Source,
    tokens: &[Token],
    holes: &HashSet<usize>,
    open: usize,
) -> bool {
    matching_close(source, tokens, holes, open).is_some_and(|close| {
        next_code(source, tokens, holes, close)
            .is_some_and(|next| text_at(source, tokens, next) == "=>")
    })
}

fn is_control_keyword(text: &str) -> bool {
    matches!(
        text,
        "if" | "while"
            | "for"
            | "foreach"
            | "switch"
            | "catch"
            | "lock"
            | "fixed"
            | "using"
            | "when"
            | "return"
            | "throw"
            | "checked"
            | "unchecked"
            | "else"
            | "do"
            | "finally"
    )
}

struct Lists<'a> {
    holes: &'a HashSet<usize>,
    generics: &'a HashSet<usize>,
}

fn check_list(
    source: &Source,
    tokens: &[Token],
    lists: &Lists<'_>,
    path: &str,
    open: usize,
    close: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut pieces = Vec::new();
    let mut commas = Vec::new();
    let mut current: Option<Piece> = None;
    let mut depth = 0i32;
    let mut angles = 0i32;
    for index in open..=close {
        if !is_code(tokens, index) || lists.holes.contains(&index) {
            continue;
        }
        let text = text_at(source, tokens, index);
        if text == "<" && lists.generics.contains(&index) {
            angles += 1;
        } else if text == ">" && lists.generics.contains(&index) && angles > 0 {
            angles -= 1;
        }
        if matches!(text, "(" | "[" | "{") {
            if depth >= 1 {
                current = extend_piece(current, index);
            }
            depth += 1;
            continue;
        }
        if matches!(text, ")" | "]" | "}") {
            depth -= 1;
            if depth >= 1 {
                current = extend_piece(current, index);
            }
            continue;
        }
        if depth == 1 && angles == 0 && text == "," {
            if let Some(piece) = current.take() {
                pieces.push(piece);
            }
            commas.push(index);
            continue;
        }
        if depth >= 1 {
            current = extend_piece(current, index);
        }
    }
    if let Some(piece) = current {
        pieces.push(piece);
    }

    let open_line = line_of(source, tokens[open].start);
    let close_line = line_of(source, tokens[close].start);
    if pieces.is_empty() && open_line != close_line {
        report(
            source,
            path,
            tokens[close].start,
            "SA1112",
            "Closing parenthesis should be on line of opening parenthesis",
            diagnostics,
        );
    }
    let Some(first) = pieces.first() else {
        return;
    };
    let first_line = line_of(source, tokens[first.start].start);
    if open_line != first_line && open_line + 1 != first_line {
        report(
            source,
            path,
            tokens[first.start].start,
            "SA1114",
            "Parameter list should follow declaration",
            diagnostics,
        );
    }
    if let Some(second) = pieces.get(1) {
        let second_line = line_of(source, tokens[second.start].start);
        if open_line == first_line && first_line != second_line {
            report(
                source,
                path,
                tokens[first.start].start,
                "SA1116",
                "The parameters should begin on the line after the declaration, whenever the parameter span across multiple lines",
                diagnostics,
            );
        }
        let same_line = first_line == second_line;
        let mut previous = first;
        for piece in pieces.iter().skip(1) {
            let start_line = line_of(source, tokens[piece.start].start);
            let previous_end = end_line(source, tokens, previous.end);
            let broken = if same_line {
                start_line != line_of(source, tokens[previous.start].start)
            } else {
                start_line == previous_end
            };
            if broken {
                report(
                    source,
                    path,
                    tokens[piece.start].start,
                    "SA1117",
                    "The parameters should all be placed on the same line or each parameter should be placed on its own line",
                    diagnostics,
                );
                break;
            }
            previous = piece;
        }
    }
    let mut previous_end = end_line(source, tokens, pieces[0].end);
    for piece in pieces.iter().skip(1) {
        let start_line = line_of(source, tokens[piece.start].start);
        if start_line > previous_end + 1 {
            report(
                source,
                path,
                tokens[piece.start].start,
                "SA1115",
                "The parameter should begin on the line after the previous parameter",
                diagnostics,
            );
        }
        previous_end = end_line(source, tokens, piece.end);
    }
    let last = pieces.last().unwrap();
    if close_line != end_line(source, tokens, last.end) {
        report(
            source,
            path,
            tokens[close].start,
            "SA1111",
            "Closing parenthesis should be on line of last parameter",
            diagnostics,
        );
    }
    for (comma, piece) in commas.iter().zip(pieces.iter()) {
        if line_of(source, tokens[*comma].start) > end_line(source, tokens, piece.end) {
            report(
                source,
                path,
                tokens[*comma].start,
                "SA1113",
                "Comma should be on the same line as previous parameter",
                diagnostics,
            );
        }
    }
    if text_at(source, tokens, open) == "(" {
        for piece in pieces.iter().skip(1) {
            if end_line(source, tokens, piece.end) > line_of(source, tokens[piece.start].start)
                && !multiline_exception(source, tokens, piece)
            {
                report(
                    source,
                    path,
                    tokens[piece.start].start,
                    "SA1118",
                    "The parameter spans multiple lines",
                    diagnostics,
                );
            }
        }
    }

    let lambda = next_code(source, tokens, lists.holes, close)
        .is_some_and(|next| text_at(source, tokens, next) == "=>");
    if !lambda
        && let Some(prev) = prev_code(source, tokens, lists.holes, open)
        && line_of(source, tokens[prev].start) != open_line
    {
        report(
            source,
            path,
            tokens[open].start,
            "SA1110",
            "Opening parenthesis or bracket should be on declaration line",
            diagnostics,
        );
    }
}

fn multiline_exception(source: &Source, tokens: &[Token], piece: &Piece) -> bool {
    let first = text_at(source, tokens, piece.start);
    if matches!(first, "new" | "[" | "{" | "delegate") {
        return true;
    }
    let mut depth = 0i32;
    let mut top = Vec::new();
    for index in piece.start..=piece.end {
        if !is_code(tokens, index) {
            continue;
        }
        let text = text_at(source, tokens, index);
        if matches!(text, "(" | "[" | "{") {
            if depth == 0 {
                top.push(text);
            }
            depth += 1;
            continue;
        }
        if matches!(text, ")" | "]" | "}") {
            depth -= 1;
            if depth == 0 {
                top.push(text);
            }
            continue;
        }
        if depth == 0 {
            top.push(text);
        }
    }
    if top.contains(&"=>") {
        return true;
    }
    let saw_call = top.windows(2).any(|pair| pair[0] == "(" && pair[1] == ")");
    let plain = top.iter().enumerate().all(|(index, text)| {
        matches!(
            *text,
            "." | "?."
                | "::"
                | "<"
                | ">"
                | "("
                | ")"
                | "typeof"
                | "nameof"
                | "default"
                | "async"
                | "static"
        ) || (*text == "!"
            && top
                .get(index + 1)
                .is_some_and(|next| matches!(*next, "." | "?.")))
            || !is_punctuation(text)
    });
    plain && saw_call && top.last().is_some_and(|text| *text == ")")
}

fn is_punctuation(text: &str) -> bool {
    text.chars()
        .all(|ch| !ch.is_ascii_alphanumeric() && ch != '_')
}

fn extend_piece(current: Option<Piece>, index: usize) -> Option<Piece> {
    Some(match current {
        Some(piece) => Piece {
            start: piece.start,
            end: index,
        },
        None => Piece {
            start: index,
            end: index,
        },
    })
}

fn matching_close(
    source: &Source,
    tokens: &[Token],
    holes: &HashSet<usize>,
    open: usize,
) -> Option<usize> {
    let mut depth = 0i32;
    for index in open..tokens.len() {
        if !is_code(tokens, index) || holes.contains(&index) {
            continue;
        }
        match text_at(source, tokens, index) {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn hole_braces(source: &Source, tokens: &[Token]) -> HashSet<usize> {
    let mut holes = HashSet::new();
    let mut strings = 0i32;
    let mut depth = 0i32;
    for (index, token) in tokens.iter().enumerate() {
        let text = source.slice(token.start, token.end);
        if text == "$\"" || text == "$@\"" || text == "@$\"" {
            strings += 1;
        } else if strings > 0 && text == "{" {
            depth += 1;
            holes.insert(index);
        } else if strings > 0 && text == "}" && depth > 0 {
            holes.insert(index);
            depth -= 1;
        } else if strings > 0 && depth == 0 && text == "\"" {
            strings -= 1;
        }
    }
    holes
}

fn prev_code(
    source: &Source,
    tokens: &[Token],
    holes: &HashSet<usize>,
    index: usize,
) -> Option<usize> {
    let mut cursor = index;
    while cursor > 0 {
        cursor -= 1;
        if is_code(tokens, cursor) && !holes.contains(&cursor) {
            let _ = source;
            return Some(cursor);
        }
    }
    None
}

fn next_code(
    source: &Source,
    tokens: &[Token],
    holes: &HashSet<usize>,
    index: usize,
) -> Option<usize> {
    let mut cursor = index + 1;
    while cursor < tokens.len() {
        if is_code(tokens, cursor) && !holes.contains(&cursor) {
            let _ = source;
            return Some(cursor);
        }
        cursor += 1;
    }
    None
}

fn is_code(tokens: &[Token], index: usize) -> bool {
    !matches!(
        tokens[index].kind,
        TokenKind::InterpolatedText | TokenKind::EndOfFile
    )
}

fn text_at<'a>(source: &'a Source, tokens: &[Token], index: usize) -> &'a str {
    let token = &tokens[index];
    source.slice(token.start, token.end)
}

fn line_of(source: &Source, byte: u32) -> u32 {
    source.line_column(byte as usize).0
}

fn end_line(source: &Source, tokens: &[Token], index: usize) -> u32 {
    let end = tokens[index].end;
    let byte = if end > tokens[index].start {
        end - 1
    } else {
        tokens[index].start
    };
    line_of(source, byte)
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
