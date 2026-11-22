//! Token spacing SA1000–SA1024 and SA1026.
//!
//! Generic brackets are told apart from comparison operators by a lookahead that
//! accepts only type-argument tokens before the closing `>`.

use std::collections::HashSet;

use crate::config::FileSettings;
use crate::lex::{Token, TokenKind, TriviaKind};
use crate::source::{Source, is_dotnet_whitespace};
use crate::whitespace::Diagnostic;

const SPACING: &str = "stylecop.csharp.spacingrules";

pub fn spacing_diagnostics(
    source: &Source,
    tokens: &[Token],
    path: &str,
    settings: &FileSettings,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if tokens.is_empty() {
        return diagnostics;
    }
    let generics = generic_indexes(source, tokens);
    let attributes = attribute_indexes(source, tokens);
    let holes = hole_braces(source, tokens);
    for index in 0..tokens.len() {
        let token = &tokens[index];
        if token.kind == TokenKind::EndOfFile {
            continue;
        }
        let text = source.slice(token.start, token.end);
        if text.is_empty() {
            continue;
        }
        let previous = if index == 0 {
            None
        } else {
            Some(&tokens[index - 1])
        };
        let next = tokens.get(index + 1);
        keyword_spacing(source, path, token, text, next, &mut diagnostics);
        if text == "," {
            comma_spacing(source, path, token, next, &mut diagnostics);
        } else if text == ";" {
            semicolon_spacing(source, path, token, previous, next, &mut diagnostics);
        } else if text == "." || text == "?." || text == "->" {
            member_spacing(source, path, token, text, &mut diagnostics);
        } else if text == "?" && is_nullable(source, tokens, index) {
            nullable_spacing(source, path, token, previous, &mut diagnostics);
        } else if generics.contains(&index) && (text == "<" || text == ">") {
            generic_spacing(source, path, token, text, next, &mut diagnostics);
        } else if is_increment(text) {
            increment_spacing(source, path, token, text, previous, next, &mut diagnostics);
        } else if is_sign(text) && is_unary(source, previous) {
            sign_spacing(source, path, token, text, previous, &mut diagnostics);
        } else if (text == "*" || text == "&") && is_unary(source, previous) {
            if followed_by_whitespace(token, next) {
                report(
                    source,
                    path,
                    token.start,
                    "SA1023",
                    &format!("Dereference symbol '{text}' should not be followed by a space"),
                    &mut diagnostics,
                );
            }
        } else if is_binary_operator(text)
            && !generics.contains(&index)
            && !is_pointer(source, token, previous)
        {
            binary_spacing(source, path, token, text, previous, &mut diagnostics);
        } else if text == "(" && followed_by_whitespace(token, next) {
            report(
                source,
                path,
                token.start,
                "SA1008",
                "Opening parenthesis should not be followed by a space.",
                &mut diagnostics,
            );
        } else if text == ")"
            && !first_in_line(source, token)
            && preceded_by_whitespace(source, token)
        {
            report(
                source,
                path,
                token.start,
                "SA1009",
                "Closing parenthesis should not be preceded by a space",
                &mut diagnostics,
            );
        } else if text == "[" || text == "]" {
            bracket_spacing(
                source,
                path,
                token,
                text,
                next,
                attributes.contains(&index),
                &mut diagnostics,
            );
        } else if text == "{"
            && !holes.contains(&index)
            && !first_in_line(source, token)
            && !preceded_by_whitespace(source, token)
        {
            report(
                source,
                path,
                token.start,
                "SA1012",
                "Opening brace should be preceded by a space",
                &mut diagnostics,
            );
        } else if text == "}" && !holes.contains(&index) {
            brace_close_spacing(source, path, token, next, &mut diagnostics);
        } else if text == ":" {
            colon_spacing(source, path, token, previous, &mut diagnostics);
        }
    }
    diagnostics.retain(|diagnostic| settings.allows(diagnostic.code, SPACING));
    diagnostics
}

fn keyword_spacing(
    source: &Source,
    path: &str,
    token: &Token,
    text: &str,
    next: Option<&Token>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if token.kind != TokenKind::Identifier {
        return;
    }
    let next_text = next
        .map(|token| source.slice(token.start, token.end))
        .unwrap_or("");
    if text == "operator" {
        if !has_required_following_space(token) {
            report(
                source,
                path,
                token.start,
                "SA1007",
                "Operator keyword should be followed by a space",
                diagnostics,
            );
        }
        return;
    }
    if text == "new" || text == "stackalloc" {
        if next_text == "[" {
            if followed_by_whitespace(token, next) || last_in_line(source, token, next) {
                report(
                    source,
                    path,
                    token.start,
                    "SA1026",
                    &format!(
                        "The keyword '{text}' should not be followed by a space or a blank line"
                    ),
                    diagnostics,
                );
            }
            return;
        }
        if next_text == "(" {
            if followed_by_whitespace(token, next) {
                report_keyword(source, path, token, text, true, diagnostics);
            }
            return;
        }
        if !has_required_following_space(token) {
            report_keyword(source, path, token, text, false, diagnostics);
        }
        return;
    }
    if text == "throw" {
        if next_text == ";" {
            if followed_by_whitespace(token, next) {
                report_keyword(source, path, token, text, true, diagnostics);
            }
        } else if !has_required_following_space(token) {
            report_keyword(source, path, token, text, false, diagnostics);
        }
        return;
    }
    if text == "return" {
        if next_text == ";" || next_text == ":" {
            if followed_by_whitespace(token, next) {
                report_keyword(source, path, token, text, true, diagnostics);
            }
        } else if !has_required_following_space(token) {
            report_keyword(source, path, token, text, false, diagnostics);
        }
        return;
    }
    if matches!(
        text,
        "default" | "typeof" | "sizeof" | "nameof" | "checked" | "unchecked"
    ) && followed_by_whitespace(token, next)
    {
        report_keyword(source, path, token, text, true, diagnostics);
        return;
    }
    let contextual = CONTEXTUAL_SPACE.contains(&text)
        && !matches!(next_text, "(" | ")" | "," | ";" | "." | "" | "]" | "}");
    if (REQUIRED_SPACE.contains(&text) || contextual) && !has_required_following_space(token) {
        report_keyword(source, path, token, text, false, diagnostics);
    }
}

fn report_keyword(
    source: &Source,
    path: &str,
    token: &Token,
    text: &str,
    disallowed: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let gap = if disallowed { " not" } else { "" };
    report(
        source,
        path,
        token.start,
        "SA1000",
        &format!("The keyword '{text}' should{gap} be followed by a space"),
        diagnostics,
    );
}

fn comma_spacing(
    source: &Source,
    path: &str,
    token: &Token,
    next: Option<&Token>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if first_in_line(source, token) || preceded_by_whitespace(source, token) {
        report(
            source,
            path,
            token.start,
            "SA1001",
            "Commas should not be preceded by whitespace",
            diagnostics,
        );
    }
    let next_text = next
        .map(|token| source.slice(token.start, token.end))
        .unwrap_or("");
    let followed = trailing_is(token, TriviaKind::Whitespace)
        || trailing_is(token, TriviaKind::EndOfLine)
        || matches!(next_text, "," | ">" | "]");
    if !followed && next.is_some() {
        report(
            source,
            path,
            token.start,
            "SA1001",
            "Commas should be followed by whitespace",
            diagnostics,
        );
    }
}

fn semicolon_spacing(
    source: &Source,
    path: &str,
    token: &Token,
    previous: Option<&Token>,
    next: Option<&Token>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(next) = next else {
        return;
    };
    let next_text = source.slice(next.start, next.end);
    let prev_text = previous
        .map(|token| source.slice(token.start, token.end))
        .unwrap_or("");
    let followed = trailing_is(token, TriviaKind::Whitespace)
        || trailing_is(token, TriviaKind::EndOfLine)
        || next_text == ")";
    if !followed {
        report(
            source,
            path,
            token.start,
            "SA1002",
            "Semicolons should be followed by a space",
            diagnostics,
        );
    }
    if !first_in_line(source, token) && prev_text != ";" && previous_ends_with_space(previous) {
        report(
            source,
            path,
            token.start,
            "SA1002",
            "Semicolons should not be preceded by a space",
            diagnostics,
        );
    }
}

fn member_spacing(
    source: &Source,
    path: &str,
    token: &Token,
    text: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !first_in_line(source, token) && preceded_by_whitespace(source, token) {
        report(
            source,
            path,
            token.start,
            "SA1019",
            &format!("Member access symbol '{text}' should not be preceded by a space"),
            diagnostics,
        );
    }
    if followed_by_whitespace(token, None) {
        report(
            source,
            path,
            token.start,
            "SA1019",
            &format!("Member access symbol '{text}' should not be followed by a space"),
            diagnostics,
        );
    }
}

fn nullable_spacing(
    source: &Source,
    path: &str,
    token: &Token,
    previous: Option<&Token>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if previous_ends_with_space(previous) || preceded_by_whitespace(source, token) {
        report(
            source,
            path,
            token.start,
            "SA1018",
            "Nullable type symbol should not be preceded by a space",
            diagnostics,
        );
    }
}

fn generic_spacing(
    source: &Source,
    path: &str,
    token: &Token,
    text: &str,
    next: Option<&Token>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let opening = text == "<";
    let name = if opening { "Opening" } else { "Closing" };
    let code = if opening { "SA1014" } else { "SA1015" };
    if !first_in_line(source, token) && preceded_by_whitespace(source, token) {
        report(
            source,
            path,
            token.start,
            code,
            &format!(
                "{} generic {} should not be preceded by a space",
                name,
                if opening { "brackets" } else { "bracket" }
            ),
            diagnostics,
        );
    }
    if opening {
        if followed_by_whitespace(token, next) {
            report(
                source,
                path,
                token.start,
                code,
                "Opening generic brackets should not be followed by a space",
                diagnostics,
            );
        }
        return;
    }
    let next_text = next
        .map(|token| source.slice(token.start, token.end))
        .unwrap_or("");
    let followed = followed_by_whitespace(token, next);
    let (allow_no_space, allow_space) = match next_text {
        "." | "," | "[" | ";" => (true, false),
        ")" | ">" | "?" | "*" | "&" => (true, true),
        "(" => (true, false),
        "" => (true, true),
        _ => (false, true),
    };
    if !last_in_line(source, token, next) && !allow_no_space && !followed {
        report(
            source,
            path,
            token.start,
            code,
            "Closing generic bracket should be followed by a space",
            diagnostics,
        );
    } else if !last_in_line(source, token, next) && !allow_space && followed {
        report(
            source,
            path,
            token.start,
            code,
            "Closing generic bracket should not be followed by a space",
            diagnostics,
        );
    }
}

fn increment_spacing(
    source: &Source,
    path: &str,
    token: &Token,
    text: &str,
    previous: Option<&Token>,
    next: Option<&Token>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let symbol = if text == "++" {
        "Increment"
    } else {
        "Decrement"
    };
    if is_expression_end(source, previous) {
        if previous_ends_with_space(previous) || preceded_by_whitespace(source, token) {
            report(
                source,
                path,
                token.start,
                "SA1003",
                &format!("Operator '{text}' should not be preceded by whitespace."),
                diagnostics,
            );
            report(
                source,
                path,
                token.start,
                "SA1020",
                &format!("{symbol} symbol '{text}' should not be preceded by a space"),
                diagnostics,
            );
        }
    } else if followed_by_whitespace(token, next) {
        report(
            source,
            path,
            token.start,
            "SA1020",
            &format!("{symbol} symbol '{text}' should not be followed by a space"),
            diagnostics,
        );
    }
}

fn sign_spacing(
    source: &Source,
    path: &str,
    token: &Token,
    text: &str,
    previous: Option<&Token>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let (code, name) = if text == "-" {
        ("SA1021", "Negative sign")
    } else {
        ("SA1022", "Positive sign")
    };
    if followed_by_whitespace(token, None) {
        report(
            source,
            path,
            token.start,
            code,
            &format!("{name} should not be followed by a space"),
            diagnostics,
        );
    }
    let prev_text = previous
        .map(|token| source.slice(token.start, token.end))
        .unwrap_or("");
    let special = matches!(prev_text, "(" | "[" | ")");
    if !first_in_line(source, token) && special && preceded_by_whitespace(source, token) {
        report(
            source,
            path,
            token.start,
            code,
            &format!("{name} should not be preceded by a space"),
            diagnostics,
        );
    } else if !first_in_line(source, token) && !special && !preceded_by_whitespace(source, token) {
        report(
            source,
            path,
            token.start,
            code,
            &format!("{name} should be preceded by a space"),
            diagnostics,
        );
    }
}

fn binary_spacing(
    source: &Source,
    path: &str,
    token: &Token,
    text: &str,
    previous: Option<&Token>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let prev = previous
        .map(|token| source.slice(token.start, token.end))
        .unwrap_or("");
    if prev == "operator" {
        return;
    }
    if !first_in_line(source, token) && !preceded_by_whitespace(source, token) {
        report(
            source,
            path,
            token.start,
            "SA1003",
            &format!("Operator '{text}' should be preceded by whitespace."),
            diagnostics,
        );
    }
    if !trailing_is(token, TriviaKind::Whitespace) && !trailing_is(token, TriviaKind::EndOfLine) {
        report(
            source,
            path,
            token.start,
            "SA1003",
            &format!("Operator '{text}' should be followed by whitespace."),
            diagnostics,
        );
    }
}

fn attribute_indexes(source: &Source, tokens: &[Token]) -> HashSet<usize> {
    let mut attributes = HashSet::new();
    let mut stack = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        let text = source.slice(token.start, token.end);
        if text == "[" {
            let prev = if index == 0 {
                ""
            } else {
                source.slice(tokens[index - 1].start, tokens[index - 1].end)
            };
            let attribute = (first_in_line(source, token)
                && closer_is_not_statement(source, tokens, index))
                || prev.is_empty()
                || matches!(prev, "(" | "," | ";" | "{" | "}" | "]");
            stack.push(attribute);
            if attribute {
                attributes.insert(index);
            }
        } else if text == "]" && stack.pop().unwrap_or(false) {
            attributes.insert(index);
        }
    }
    attributes
}

fn closer_is_not_statement(source: &Source, tokens: &[Token], open: usize) -> bool {
    let mut depth = 0i32;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        let text = source.slice(token.start, token.end);
        if text == "[" {
            depth += 1;
        } else if text == "]" {
            depth -= 1;
            if depth == 0 {
                let next_text = tokens
                    .get(index + 1)
                    .map(|token| source.slice(token.start, token.end))
                    .unwrap_or("");
                return next_text != ";";
            }
        }
    }
    true
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

fn bracket_spacing(
    source: &Source,
    path: &str,
    token: &Token,
    text: &str,
    next: Option<&Token>,
    attribute: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if text == "[" {
        if attribute && followed_by_whitespace(token, next) {
            report(
                source,
                path,
                token.start,
                "SA1016",
                "Opening attribute brackets should not be followed by a space",
                diagnostics,
            );
        } else if !attribute
            && !first_in_line(source, token)
            && preceded_by_whitespace(source, token)
            && next_is_not_implicit_array(source, token)
        {
            report(
                source,
                path,
                token.start,
                "SA1010",
                "Opening square brackets should not be preceded by a space",
                diagnostics,
            );
        }
        return;
    }
    if attribute && preceded_by_whitespace(source, token) {
        report(
            source,
            path,
            token.start,
            "SA1017",
            "Closing attribute brackets should not be preceded by a space",
            diagnostics,
        );
        return;
    }
    if preceded_by_whitespace(source, token) && !first_in_line(source, token) {
        report(
            source,
            path,
            token.start,
            "SA1011",
            "Closing square bracket should not be preceded by a space",
            diagnostics,
        );
    }
    let next_text = next
        .map(|token| source.slice(token.start, token.end))
        .unwrap_or("");
    if next_text == "{"
        && !followed_by_whitespace(token, next)
        && !last_in_line(source, token, next)
    {
        report(
            source,
            path,
            token.start,
            "SA1011",
            "Closing square bracket should be followed by a space",
            diagnostics,
        );
    }
}

fn brace_close_spacing(
    source: &Source,
    path: &str,
    token: &Token,
    next: Option<&Token>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !first_in_line(source, token) && !preceded_by_whitespace(source, token) {
        report(
            source,
            path,
            token.start,
            "SA1013",
            "Closing brace should be preceded by a space",
            diagnostics,
        );
    }
    let next_text = next
        .map(|token| source.slice(token.start, token.end))
        .unwrap_or("");
    if matches!(next_text, "else" | "catch" | "finally" | "while" | "{")
        && !followed_by_whitespace(token, next)
        && !last_in_line(source, token, next)
    {
        report(
            source,
            path,
            token.start,
            "SA1013",
            "Closing brace should be followed by a space",
            diagnostics,
        );
    }
}

fn colon_spacing(
    source: &Source,
    path: &str,
    token: &Token,
    previous: Option<&Token>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let prev = previous
        .map(|token| source.slice(token.start, token.end))
        .unwrap_or("");
    let literal = prev.chars().next().is_some_and(|ch| ch.is_ascii_digit())
        || prev.starts_with('"')
        || prev.starts_with('\'');
    let before_prev = previous
        .map(|token| source.text[..token.start as usize].trim_end())
        .unwrap_or("");
    if literal && ends_with_word(before_prev, "case") && preceded_by_whitespace(source, token) {
        report(
            source,
            path,
            token.start,
            "SA1024",
            "Colon should not be preceded by a space",
            diagnostics,
        );
    }
}

fn ends_with_word(text: &str, word: &str) -> bool {
    let Some(rest) = text.strip_suffix(word) else {
        return false;
    };
    rest.chars()
        .next_back()
        .is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_')
}

fn next_is_not_implicit_array(source: &Source, token: &Token) -> bool {
    let before = source.text[..token.start as usize].trim_end();
    !(ends_with_word(before, "new") || ends_with_word(before, "stackalloc"))
}

fn is_pointer(source: &Source, token: &Token, previous: Option<&Token>) -> bool {
    let Some(previous) = previous else {
        return false;
    };
    let prev = source.slice(previous.start, previous.end);
    !preceded_by_whitespace(source, token)
        && (TYPE_KEYWORDS.contains(&prev) || matches!(prev, ">" | "]"))
}

fn is_nullable(source: &Source, tokens: &[Token], index: usize) -> bool {
    let token = &tokens[index];
    if source.slice(token.start, token.end) != "?" {
        return false;
    }
    let mut depth_paren = 0i32;
    let mut depth_bracket = 0i32;
    let mut depth_brace = 0i32;
    for later in tokens.iter().skip(index + 1) {
        if later.kind == TokenKind::EndOfFile {
            break;
        }
        let text = source.slice(later.start, later.end);
        match text {
            "(" => depth_paren += 1,
            ")" if depth_paren == 0 => break,
            ")" => depth_paren -= 1,
            "[" => depth_bracket += 1,
            "]" if depth_bracket == 0 => break,
            "]" => depth_bracket -= 1,
            "{" => depth_brace += 1,
            "}" if depth_brace == 0 => break,
            "}" => depth_brace -= 1,
            ":" if depth_paren == 0 && depth_bracket == 0 && depth_brace == 0 => return false,
            ";" => break,
            _ => {}
        }
    }
    true
}

fn is_unary(source: &Source, previous: Option<&Token>) -> bool {
    !is_expression_end(source, previous)
}

fn is_expression_end(source: &Source, previous: Option<&Token>) -> bool {
    let Some(previous) = previous else {
        return false;
    };
    let text = source.slice(previous.start, previous.end);
    previous.kind == TokenKind::Identifier
        || previous.kind == TokenKind::String
        || previous.kind == TokenKind::Char
        || text.chars().next().is_some_and(|ch| ch.is_ascii_digit())
        || matches!(
            text,
            ")" | "]" | "++" | "--" | "!" | "null" | "true" | "false" | "this" | "base"
        )
}

fn generic_indexes(source: &Source, tokens: &[Token]) -> HashSet<usize> {
    let mut generic = HashSet::new();
    let mut index = 0;
    while index < tokens.len() {
        if source.slice(tokens[index].start, tokens[index].end) == "<"
            && can_start_generic(source, tokens, index)
            && let Some(marked) = scan_type_arguments(source, tokens, index)
        {
            generic.extend(marked);
        }
        index += 1;
    }
    generic
}

fn can_start_generic(source: &Source, tokens: &[Token], index: usize) -> bool {
    if index == 0 {
        return false;
    }
    let text = source.slice(tokens[index - 1].start, tokens[index - 1].end);
    tokens[index - 1].kind == TokenKind::Identifier
        || TYPE_KEYWORDS.contains(&text)
        || matches!(text, ">" | "]" | "?")
}

fn scan_type_arguments(source: &Source, tokens: &[Token], start: usize) -> Option<Vec<usize>> {
    let mut marked = vec![start];
    let mut depth = 1i32;
    let mut paren = 0i32;
    let mut bracket = 0i32;
    let mut index = start + 1;
    while index < tokens.len() && depth > 0 {
        let text = source.slice(tokens[index].start, tokens[index].end);
        if tokens[index].kind == TokenKind::EndOfFile {
            return None;
        }
        match text {
            "<" => {
                depth += 1;
                marked.push(index);
            }
            ">" => {
                depth -= 1;
                marked.push(index);
            }
            "(" => paren += 1,
            ")" => {
                if paren == 0 {
                    return None;
                }
                paren -= 1;
            }
            "[" => bracket += 1,
            "]" => {
                if bracket == 0 {
                    return None;
                }
                bracket -= 1;
            }
            "," | "." | "?" | "*" | "&" | "::" => {}
            _ if tokens[index].kind == TokenKind::Identifier || TYPE_KEYWORDS.contains(&text) => {}
            _ => return None,
        }
        index += 1;
    }
    if depth == 0 { Some(marked) } else { None }
}

fn has_required_following_space(token: &Token) -> bool {
    matches!(
        token.trailing.first().map(|trivia| trivia.kind),
        Some(TriviaKind::Whitespace | TriviaKind::EndOfLine)
    )
}

fn followed_by_whitespace(token: &Token, next: Option<&Token>) -> bool {
    if let Some(trivia) = token.trailing.first() {
        return trivia.kind == TriviaKind::Whitespace;
    }
    next.and_then(|token| token.leading.first())
        .is_some_and(|trivia| trivia.kind == TriviaKind::Whitespace)
}

fn trailing_is(token: &Token, kind: TriviaKind) -> bool {
    token
        .trailing
        .first()
        .is_some_and(|trivia| trivia.kind == kind)
}

fn previous_ends_with_space(previous: Option<&Token>) -> bool {
    previous.is_some_and(|token| {
        token
            .trailing
            .last()
            .is_some_and(|trivia| trivia.kind == TriviaKind::Whitespace)
    })
}

fn last_in_line(source: &Source, token: &Token, next: Option<&Token>) -> bool {
    if trailing_is(token, TriviaKind::EndOfLine) {
        return true;
    }
    let Some(next) = next else {
        return true;
    };
    source.line_column(token.end as usize).0 < source.line_column(next.start as usize).0
}

fn first_in_line(source: &Source, token: &Token) -> bool {
    let full = token
        .leading
        .first()
        .map(|trivia| trivia.start)
        .unwrap_or(token.start);
    if source.line_column(full as usize).1 == 1 {
        return true;
    }
    source.line_column(full as usize).0 != source.line_column(token.start as usize).0
}

fn preceded_by_whitespace(source: &Source, token: &Token) -> bool {
    if token.start == 0 {
        return false;
    }
    source.text[..token.start as usize]
        .chars()
        .next_back()
        .is_some_and(is_dotnet_whitespace)
}

fn is_increment(text: &str) -> bool {
    text == "++" || text == "--"
}

fn is_sign(text: &str) -> bool {
    text == "+" || text == "-"
}

fn is_binary_operator(text: &str) -> bool {
    matches!(
        text,
        "+" | "-"
            | "*"
            | "/"
            | "%"
            | "="
            | "=="
            | "!="
            | "<="
            | ">="
            | "&&"
            | "||"
            | "&"
            | "|"
            | "^"
            | "??"
            | "+="
            | "-="
            | "*="
            | "/="
            | "%="
            | "&="
            | "|="
            | "^="
            | "<<"
            | "<<="
            | ">>="
            | "=>"
            | "??="
    )
}

const REQUIRED_SPACE: &[&str] = &[
    "case", "catch", "fixed", "for", "foreach", "if", "is", "lock", "switch", "using", "while",
];

const CONTEXTUAL_SPACE: &[&str] = &[
    "and", "await", "from", "group", "in", "into", "join", "let", "not", "or", "orderby", "out",
    "ref", "select", "where", "yield",
];

const TYPE_KEYWORDS: &[&str] = &[
    "bool",
    "byte",
    "char",
    "decimal",
    "double",
    "float",
    "int",
    "long",
    "object",
    "sbyte",
    "short",
    "string",
    "uint",
    "ulong",
    "ushort",
    "void",
    "nint",
    "nuint",
    "dynamic",
    "var",
    "class",
    "struct",
    "unmanaged",
    "notnull",
    "new",
    "in",
    "out",
];

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
