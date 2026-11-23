//! Syntax rules that are not member order or declaration-keyword order.
//!
//! Readability rules stay in the readability category. File shape, parentheses,
//! precedence, and trailing commas are maintainability rules.

use crate::declarations::{Kind, Parsed, first_in_line};
use crate::lex::{Token, TokenKind, TriviaKind};
use crate::source::Source;
use crate::whitespace::Diagnostic;

const READABILITY: &str = "stylecop.csharp.readabilityrules";
const MAINTAINABILITY: &str = "stylecop.csharp.maintainabilityrules";

pub fn shape_diagnostics(
    source: &Source,
    tokens: &[Token],
    path: &str,
    parsed: &Parsed,
    utf8_bom: bool,
) -> Vec<(Diagnostic, &'static str)> {
    let mut diagnostics = Vec::new();
    statements(source, tokens, path, parsed, &mut diagnostics);
    embedded_comments(source, tokens, path, parsed, &mut diagnostics);
    regions(source, tokens, path, parsed, &mut diagnostics);
    empty_strings(source, tokens, path, parsed, &mut diagnostics);
    constraints(source, tokens, path, parsed, &mut diagnostics);
    constructor_initializers(source, path, parsed, &mut diagnostics);
    combined_fields(source, path, parsed, &mut diagnostics);
    attributes(source, tokens, path, parsed, &mut diagnostics);
    enums(source, path, parsed, &mut diagnostics);
    initializers(source, path, parsed, &mut diagnostics);
    single_type(source, path, parsed, &mut diagnostics);
    single_namespace(source, path, parsed, &mut diagnostics);
    utf8(source, tokens, path, utf8_bom, &mut diagnostics);
    diagnostics
}

fn statements(
    source: &Source,
    tokens: &[Token],
    path: &str,
    parsed: &Parsed,
    diagnostics: &mut Vec<(Diagnostic, &'static str)>,
) {
    let generics = crate::spacing::generic_indexes(source, tokens);
    for (start, end) in outermost(&parsed.blocks) {
        let mut scan = Scan {
            source,
            tokens,
            code: &parsed.code,
            generics: &generics,
            i: start,
            end,
            path,
            diagnostics,
        };
        scan.statement_list();
    }
}

fn outermost(blocks: &[(usize, usize)]) -> Vec<(usize, usize)> {
    blocks
        .iter()
        .copied()
        .filter(|(start, end)| {
            !blocks.iter().any(|(outer, outer_end)| {
                *outer <= *start && *outer_end >= *end && (*outer, *outer_end) != (*start, *end)
            })
        })
        .collect()
}

struct Scan<'a> {
    source: &'a Source,
    tokens: &'a [Token],
    code: &'a [usize],
    generics: &'a std::collections::HashSet<usize>,
    i: usize,
    end: usize,
    path: &'a str,
    diagnostics: &'a mut Vec<(Diagnostic, &'static str)>,
}

struct Stmt {
    start: u32,
    end_line: u32,
    empty: bool,
    labeled_empty: bool,
}

impl<'a> Scan<'a> {
    fn statement_list(&mut self) {
        let mut statements = Vec::new();
        while self.i < self.end && self.at() != Some("}") {
            let before = self.i;
            if let Some(statement) = self.statement() {
                statements.push(statement);
            }
            if self.i == before {
                self.bump();
            }
        }
        for pair in statements.windows(2) {
            if pair[0].end_line == self.line_of(pair[1].start) {
                self.report(
                    pair[1].start,
                    "SA1107",
                    "Code should not contain multiple statements on one line",
                    READABILITY,
                );
            }
        }
        for (index, statement) in statements.iter().enumerate() {
            if !statement.empty {
                continue;
            }
            if statement.labeled_empty && label_is_last(index, &statements) {
                continue;
            }
            self.report(
                statement.start,
                "SA1106",
                "Code should not contain empty statements",
                READABILITY,
            );
        }
    }

    fn statement(&mut self) -> Option<Stmt> {
        let start = self.byte();
        let start_line = self.line_of(start);
        if self.at() == Some(";") {
            self.bump();
            return Some(Stmt {
                start,
                end_line: start_line,
                empty: true,
                labeled_empty: false,
            });
        }
        if self.at() == Some("{") {
            self.nested_block();
            return Some(Stmt {
                start,
                end_line: self.previous_line(),
                empty: false,
                labeled_empty: false,
            });
        }
        if self.is_label() {
            self.bump();
            self.bump();
            let inner = self.statement();
            let labeled_empty = inner.as_ref().is_some_and(|inner| inner.empty);
            return Some(Stmt {
                start,
                end_line: inner.map(|inner| inner.end_line).unwrap_or(start_line),
                empty: false,
                labeled_empty,
            });
        }
        let keyword = self.at().unwrap_or("").to_owned();
        match keyword.as_str() {
            "if" => {
                self.bump();
                self.condition();
                self.statement();
                if self.at() == Some("else") {
                    self.bump();
                    self.statement();
                }
            }
            "for" | "foreach" | "while" | "lock" | "fixed" => {
                self.bump();
                self.condition();
                self.statement();
            }
            "switch" => {
                self.bump();
                self.condition();
                // Switch sections are not blocks, so SA1107 does not see them.
                if self.at() == Some("{") {
                    self.skip_pair("{", "}");
                }
            }
            "using" if self.lookahead(1) == Some("(") => {
                self.bump();
                self.skip_pair("(", ")");
                self.statement();
            }
            "do" => {
                self.bump();
                self.statement();
                if self.at() == Some("while") {
                    self.bump();
                    self.skip_pair("(", ")");
                    if self.at() == Some(";") {
                        self.bump();
                    }
                }
            }
            "try" => {
                self.bump();
                self.statement();
                while self.at() == Some("catch") {
                    self.bump();
                    if self.at() == Some("(") {
                        self.skip_pair("(", ")");
                    }
                    self.statement();
                }
                if self.at() == Some("finally") {
                    self.bump();
                    self.statement();
                }
            }
            "checked" | "unchecked" | "unsafe" => {
                self.bump();
                if self.at() == Some("(") {
                    self.skip_pair("(", ")");
                    self.finish_expression_statement();
                } else {
                    self.statement();
                }
            }
            "return" | "throw" | "break" | "continue" | "goto" | "yield" | "const" => {
                self.finish_expression_statement();
            }
            _ if self.local_function() => {}
            _ => self.finish_expression_statement(),
        }
        Some(Stmt {
            start,
            end_line: self.previous_line(),
            empty: false,
            labeled_empty: false,
        })
    }

    fn condition(&mut self) {
        if self.at() != Some("(") {
            return;
        }
        let open = self.i;
        let close = self.matching_from(self.i, "(", ")");
        self.check_expression(open + 1, close);
        self.i = (close + 1).min(self.end);
    }

    fn finish_expression_statement(&mut self) {
        let mut paren = 0i32;
        let mut bracket = 0i32;
        let mut angle = 0i32;
        let mut value_at = None;
        if matches!(self.at(), Some("return" | "throw" | "yield")) {
            self.bump();
            value_at = Some(self.i);
        }
        while self.i < self.end {
            match self.at() {
                Some("(") => paren += 1,
                Some(")") => paren -= 1,
                Some("[") => bracket += 1,
                Some("]") => bracket -= 1,
                Some("<") if paren == 0 && bracket == 0 => angle += 1,
                Some(">") if paren == 0 && bracket == 0 && angle > 0 => angle -= 1,
                Some("=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=")
                    if paren == 0 && bracket == 0 && angle == 0 =>
                {
                    self.bump();
                    value_at = Some(self.i);
                    continue;
                }
                Some("{") if bracket == 0 && angle == 0 => {
                    if self.opens_statement() {
                        self.nested_block();
                    } else {
                        self.skip_initializer();
                    }
                    continue;
                }
                Some(";") if paren == 0 && bracket == 0 && angle == 0 => {
                    if let Some(start) = value_at {
                        self.check_expression(start, self.i);
                    } else if !matches!(self.text(self.i.saturating_sub(1).min(self.end)), ";") {
                        // A return or throw statement is itself the expression.
                    }
                    self.bump();
                    return;
                }
                _ => {}
            }
            self.i += 1;
        }
    }

    fn local_function(&mut self) -> bool {
        let saved = self.i;
        self.skip_modifiers();
        if !self.skip_type_tokens() || !self.at().is_some_and(is_name) {
            self.i = saved;
            return false;
        }
        self.bump();
        if self.at() == Some("<") {
            self.skip_angles();
        }
        if self.at() != Some("(") {
            self.i = saved;
            return false;
        }
        self.skip_pair("(", ")");
        while self.at() == Some("where") {
            self.bump();
            while self.i < self.end && !matches!(self.at(), Some("{" | "=>" | "where")) {
                self.bump();
            }
        }
        if self.at() == Some("{") {
            self.nested_block();
            true
        } else if self.at() == Some("=>") {
            self.bump();
            self.finish_expression_statement();
            true
        } else {
            self.i = saved;
            false
        }
    }

    fn skip_initializer(&mut self) {
        let open_line = self.line_of(self.byte());
        let close = self.matching_brace();
        let close_line = if close < self.code.len() {
            self.line_of(self.tokens[self.code[close]].start)
        } else {
            open_line
        };
        let mut last_at = None;
        let mut awaiting = true;
        let mut paren = 0i32;
        let mut bracket = 0i32;
        let mut brace = 0i32;
        let mut angle = 0i32;
        self.bump();
        while self.i < close {
            match self.at().unwrap_or("") {
                "(" => paren += 1,
                ")" => paren -= 1,
                "[" => bracket += 1,
                "]" => bracket -= 1,
                "{" => brace += 1,
                "}" => brace -= 1,
                "<" if paren == 0 && bracket == 0 && brace == 0 => angle += 1,
                ">" if paren == 0 && bracket == 0 && brace == 0 && angle > 0 => angle -= 1,
                "," if paren == 0 && bracket == 0 && brace == 0 && angle == 0 => awaiting = true,
                _ if paren == 0 && bracket == 0 && brace == 0 && angle == 0 && awaiting => {
                    last_at = Some(self.byte());
                    awaiting = false;
                }
                _ => {}
            }
            self.i += 1;
        }
        if open_line != close_line
            && !awaiting
            && let Some(last_at) = last_at
        {
            self.report(
                last_at,
                "SA1413",
                "Use trailing comma in multi-line initializers",
                MAINTAINABILITY,
            );
        }
        self.i = close;
        if self.at() == Some("}") {
            self.bump();
        }
    }

    fn nested_block(&mut self) {
        let close = self.matching_brace();
        self.bump();
        let saved = self.end;
        self.end = close;
        self.statement_list();
        self.end = saved;
        self.i = close;
        if self.at() == Some("}") {
            self.bump();
        }
    }

    fn check_expression(&mut self, start: usize, end: usize) {
        let saved_i = self.i;
        let saved_end = self.end;
        self.i = start;
        self.end = end;
        if self.i < self.end {
            let expr = self.parse_expr(0);
            walk_expr(
                &expr,
                ParentExpr::Other,
                self.source,
                self.path,
                self.diagnostics,
            );
        }
        self.i = saved_i;
        self.end = saved_end;
    }

    fn parse_expr(&mut self, min: u8) -> Expr {
        let mut left = self.parse_unary();
        while let Some(op) = self.binary_op() {
            let prec = precedence(op);
            if prec < min {
                break;
            }
            if op == Op::Cond {
                let at = left.at();
                self.bump();
                let then_branch = self.parse_expr(0);
                if self.at() == Some(":") {
                    self.bump();
                }
                let else_branch = self.parse_expr(prec);
                left = Expr::Conditional {
                    at,
                    condition: Box::new(left),
                    then_branch: Box::new(then_branch),
                    else_branch: Box::new(else_branch),
                };
                continue;
            }
            let op_at = self.byte();
            self.consume_op(op);
            let right_prec = if op == Op::Assign { prec } else { prec + 1 };
            let right = self.parse_expr(right_prec);
            left = Expr::Binary {
                op,
                op_at,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        left
    }

    fn parse_unary(&mut self) -> Expr {
        let start = self.byte();
        if matches!(
            self.at(),
            Some("+" | "-" | "!" | "~" | "++" | "--" | "&" | "*")
        ) {
            self.bump();
            return Expr::Unary(Box::new(self.parse_unary()));
        }
        if self.at() == Some("(") {
            return self.parse_paren_or_cast();
        }
        self.parse_primary(start)
    }

    fn parse_paren_or_cast(&mut self) -> Expr {
        let open = self.byte();
        let close_index = self.matching_from(self.i, "(", ")");
        let keep = self.comma_splits_parentheses(self.i, close_index)
            || self.next_keeps_parentheses(close_index);
        if close_index + 1 < self.end && self.text(close_index + 1) == "=>" {
            self.i = (close_index + 1).min(self.end);
            return self.finish_lambda_or_empty(open);
        }
        let inner_is_plain_name = self.plain_type(self.i + 1, close_index);
        if inner_is_plain_name && self.starts_unary_at(close_index + 1) {
            self.i = (close_index + 1).min(self.end);
            return Expr::Cast(Box::new(self.parse_unary()));
        }
        self.bump();
        if self.at() == Some(")") {
            self.bump();
            return self.finish_lambda_or_empty(open);
        }
        let inner = self.parse_expr(0);
        if self.at() == Some(")") {
            self.bump();
        }
        if self.at() == Some("=>") {
            return self.finish_lambda_or_empty(open);
        }
        let paren = Expr::Paren {
            at: open,
            inner: Box::new(inner),
            keep,
        };
        self.parse_postfix(paren)
    }

    fn finish_lambda_or_empty(&mut self, at: u32) -> Expr {
        if self.at() == Some("=>") {
            self.bump();
            if self.at() == Some("{") {
                self.skip_pair("{", "}");
            } else if self.i < self.end {
                let _ = self.parse_expr(0);
            }
        }
        Expr::Other { at }
    }

    fn parse_primary(&mut self, start: u32) -> Expr {
        if self.i >= self.end {
            return Expr::Other { at: start };
        }
        self.bump();
        self.parse_postfix(Expr::Other { at: start })
    }

    fn parse_postfix(&mut self, mut expr: Expr) -> Expr {
        loop {
            match self.at() {
                Some("(") => {
                    self.skip_pair("(", ")");
                    expr = Expr::Call(Box::new(expr));
                }
                Some("[") => {
                    self.skip_pair("[", "]");
                    expr = Expr::Other { at: expr.at() };
                }
                Some("?") if self.lookahead(1) == Some("[") => {
                    self.bump();
                    self.skip_pair("[", "]");
                    expr = Expr::Other { at: expr.at() };
                }
                Some(".") | Some("?.") | Some("!") => {
                    self.bump();
                    if self.at().is_some_and(is_name) {
                        self.bump();
                    }
                    expr = Expr::Member(Box::new(expr));
                }
                Some("<") if self.generics.contains(&self.code[self.i]) => {
                    self.skip_angles();
                    expr = Expr::Other { at: expr.at() };
                }
                _ => break,
            }
        }
        expr
    }

    fn plain_type(&self, start: usize, end: usize) -> bool {
        if start >= end {
            return false;
        }
        let mut i = start;
        let mut words = 0;
        while i < end {
            let text = self.text(i);
            if is_name(text) || matches!(text, "." | "::" | "?" | "*" | "<" | ">" | "[" | "]" | ",")
            {
                if is_name(text) {
                    words += 1;
                }
                i += 1;
            } else {
                return false;
            }
        }
        words > 0
    }

    fn starts_unary_at(&self, index: usize) -> bool {
        if index >= self.end {
            return false;
        }
        let text = self.text(index);
        matches!(
            text,
            "+" | "-"
                | "!"
                | "~"
                | "++"
                | "--"
                | "&"
                | "*"
                | "("
                | "this"
                | "base"
                | "new"
                | "typeof"
                | "nameof"
                | "default"
                | "checked"
                | "unchecked"
                | "null"
                | "true"
                | "false"
                | "sizeof"
                | "delegate"
                | "await"
        ) || is_name(text)
            || text.starts_with('"')
            || text.starts_with('\'')
            || matches!(
                self.tokens[self.code[index]].kind,
                TokenKind::String | TokenKind::Char
            )
    }

    fn binary_op(&self) -> Option<Op> {
        let text = self.at()?;
        Some(match text {
            "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>=" => {
                Op::Assign
            }
            "??" => Op::Coalesce,
            "||" => Op::Or,
            "&&" => Op::And,
            "|" => Op::BitOr,
            "^" => Op::BitXor,
            "&" => Op::BitAnd,
            "==" | "!=" => Op::Eq,
            "is" | "as" => Op::Rel,
            "<=" | ">=" => Op::Rel,
            "<" | ">" if !self.generics.contains(&self.code[self.i]) => Op::Rel,
            "?" if !matches!(self.lookahead(1), Some("[" | "." | "?")) => Op::Cond,
            "<<" => Op::Shift,
            "+" | "-" => Op::Add,
            "*" | "/" | "%" => Op::Mul,
            _ => return None,
        })
    }

    fn consume_op(&mut self, op: Op) {
        let is_pattern = self.at() == Some("is");
        self.bump();
        if is_pattern && self.at() == Some("not") {
            self.bump();
        }
        if op == Op::Shift && self.at() == Some(">") {
            self.bump();
        }
    }

    fn opens_statement(&self) -> bool {
        if self.i == 0 {
            return false;
        }
        let previous = self.text(self.i - 1);
        if matches!(
            previous,
            "else"
                | "do"
                | "try"
                | "finally"
                | "checked"
                | "unchecked"
                | "catch"
                | "=>"
                | "delegate"
        ) {
            return true;
        }
        if previous != ")" {
            return false;
        }
        let mut depth = 0i32;
        for cursor in (0..self.i).rev() {
            match self.text(cursor) {
                ")" => depth += 1,
                "(" => {
                    depth -= 1;
                    if depth == 0 {
                        let keyword = cursor.checked_sub(1).map(|index| self.text(index));
                        return matches!(
                            keyword,
                            Some(
                                "if" | "for"
                                    | "foreach"
                                    | "while"
                                    | "lock"
                                    | "fixed"
                                    | "switch"
                                    | "catch"
                                    | "when"
                                    | "using"
                            )
                        );
                    }
                }
                _ => {}
            }
        }
        false
    }

    fn comma_splits_parentheses(&self, open: usize, close: usize) -> bool {
        let mut depth = 0i32;
        for index in open + 1..close {
            match self.text(index) {
                "(" | "[" | "<" => depth += 1,
                ")" | "]" | ">" if depth > 0 => depth -= 1,
                "," if depth == 0 => return true,
                _ => {}
            }
        }
        false
    }

    fn next_keeps_parentheses(&self, close: usize) -> bool {
        let Some(next) = (close + 1 < self.end).then(|| self.text(close + 1)) else {
            return false;
        };
        matches!(
            next,
            "." | "?."
                | "?"
                | ":"
                | "??"
                | "&&"
                | "||"
                | "is"
                | "as"
                | "["
                | "=>"
                | "=="
                | "!="
                | "<"
                | ">"
                | "<="
                | ">="
                | "+"
                | "-"
                | "*"
                | "/"
                | "%"
        )
    }

    fn is_label(&self) -> bool {
        self.at().is_some_and(is_name)
            && self.lookahead(1) == Some(":")
            && self.lookahead(2) != Some(":")
    }

    fn skip_modifiers(&mut self) {
        while self.at().is_some_and(|text| {
            matches!(
                text,
                "public"
                    | "private"
                    | "protected"
                    | "internal"
                    | "static"
                    | "async"
                    | "unsafe"
                    | "extern"
                    | "new"
                    | "partial"
                    | "readonly"
                    | "volatile"
                    | "ref"
            )
        }) {
            self.bump();
        }
    }

    fn skip_type_tokens(&mut self) -> bool {
        if self.at() == Some("(") {
            self.skip_pair("(", ")");
        } else if self.at().is_some_and(is_name) {
            self.bump();
        } else {
            return false;
        }
        loop {
            match self.at() {
                Some("<") => self.skip_angles(),
                Some("[") => self.skip_pair("[", "]"),
                Some("?") | Some("*") => self.bump(),
                Some(".") | Some("::") => {
                    self.bump();
                    if self.at().is_some_and(is_name) {
                        self.bump();
                    }
                }
                _ => break,
            }
        }
        true
    }

    fn skip_pair(&mut self, open: &str, close: &str) {
        let end = self.matching_from(self.i, open, close);
        self.i = (end + 1).min(self.end);
    }

    fn skip_angles(&mut self) {
        let mut angle = 0i32;
        let mut paren = 0i32;
        while self.i < self.end {
            match self.at().unwrap_or("") {
                "<" if paren == 0 => angle += 1,
                ">" if paren == 0 => {
                    angle -= 1;
                    self.bump();
                    if angle == 0 {
                        return;
                    }
                    continue;
                }
                "(" => paren += 1,
                ")" => paren -= 1,
                _ => {}
            }
            self.bump();
        }
    }

    fn matching_brace(&self) -> usize {
        self.matching_from(self.i, "{", "}")
    }

    fn matching_from(&self, start: usize, open: &str, close: &str) -> usize {
        let mut depth = 0i32;
        let mut i = start;
        while i < self.end {
            let text = self.text(i);
            if text == open {
                depth += 1;
            } else if text == close {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            i += 1;
        }
        self.end.saturating_sub(1)
    }

    fn at(&self) -> Option<&str> {
        (self.i < self.end).then(|| self.text(self.i))
    }

    fn lookahead(&self, offset: usize) -> Option<&str> {
        let index = self.i + offset;
        (index < self.end).then(|| self.text(index))
    }

    fn text(&self, index: usize) -> &str {
        let token = &self.tokens[self.code[index]];
        self.source.slice(token.start, token.end)
    }

    fn bump(&mut self) {
        if self.i < self.end {
            self.i += 1;
        }
    }

    fn byte(&self) -> u32 {
        if self.i >= self.end {
            return 0;
        }
        self.tokens[self.code[self.i]].start
    }

    fn previous_line(&self) -> u32 {
        if self.i == 0 {
            return 1;
        }
        let index = self
            .i
            .saturating_sub(1)
            .min(self.code.len().saturating_sub(1));
        let token = &self.tokens[self.code[index]];
        let byte = token.end.saturating_sub(1);
        self.source.line_column(byte as usize).0
    }

    fn line_of(&self, byte: u32) -> u32 {
        self.source.line_column(byte as usize).0
    }

    fn report(&mut self, byte: u32, code: &'static str, message: &str, category: &'static str) {
        let (line, column) = self.source.line_column(byte as usize);
        self.diagnostics.push((
            Diagnostic {
                path: self.path.to_owned(),
                line,
                column,
                code,
                message: message.to_owned(),
            },
            category,
        ));
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Op {
    Assign,
    Coalesce,
    Cond,
    Or,
    And,
    BitOr,
    BitXor,
    BitAnd,
    Eq,
    Rel,
    Shift,
    Add,
    Mul,
}

fn precedence(op: Op) -> u8 {
    match op {
        Op::Mul => 10,
        Op::Add => 9,
        Op::Shift => 8,
        Op::Rel => 7,
        Op::Eq => 6,
        Op::BitAnd => 5,
        Op::BitXor => 4,
        Op::BitOr => 3,
        Op::And => 2,
        Op::Or => 1,
        Op::Coalesce => 1,
        Op::Cond => 1,
        Op::Assign => 0,
    }
}

enum Expr {
    Paren {
        at: u32,
        inner: Box<Expr>,
        keep: bool,
    },
    Binary {
        op: Op,
        op_at: u32,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Unary(Box<Expr>),
    Cast(Box<Expr>),
    Call(Box<Expr>),
    Member(Box<Expr>),
    Conditional {
        at: u32,
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Box<Expr>,
    },
    Other {
        at: u32,
    },
}

impl Expr {
    fn at(&self) -> u32 {
        match self {
            Self::Paren { at, .. }
            | Self::Binary { op_at: at, .. }
            | Self::Conditional { at, .. }
            | Self::Other { at } => *at,
            Self::Unary(inner) | Self::Cast(inner) | Self::Call(inner) | Self::Member(inner) => {
                inner.at()
            }
        }
    }
}

enum ParentExpr {
    Expression,
    Assignment,
    Member,
    Other,
}

fn walk_expr(
    expr: &Expr,
    parent: ParentExpr,
    source: &Source,
    path: &str,
    diagnostics: &mut Vec<(Diagnostic, &'static str)>,
) {
    match expr {
        Expr::Paren { at, inner, keep } => {
            if !keep && paren_is_unnecessary(inner, parent) {
                report_at(
                    source,
                    path,
                    *at,
                    "SA1119",
                    "Statement should not use unnecessary parenthesis",
                    MAINTAINABILITY,
                    diagnostics,
                );
            }
            walk_expr(inner, ParentExpr::Expression, source, path, diagnostics);
        }
        Expr::Binary {
            op, left, right, ..
        } => {
            if matches!(op, Op::Add | Op::Mul | Op::Shift) {
                report_arithmetic(left, *op, source, path, diagnostics);
                report_arithmetic(right, *op, source, path, diagnostics);
            }
            if matches!(op, Op::And | Op::Or) {
                report_conditional(left, *op, source, path, diagnostics);
                report_conditional(right, *op, source, path, diagnostics);
            }
            let child_parent = if *op == Op::Assign {
                ParentExpr::Assignment
            } else {
                ParentExpr::Expression
            };
            walk_expr(left, child_parent, source, path, diagnostics);
            walk_expr(right, ParentExpr::Expression, source, path, diagnostics);
        }
        Expr::Unary(inner) | Expr::Cast(inner) | Expr::Call(inner) => {
            walk_expr(inner, ParentExpr::Expression, source, path, diagnostics);
        }
        Expr::Conditional {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            walk_expr(condition, ParentExpr::Expression, source, path, diagnostics);
            walk_expr(
                then_branch,
                ParentExpr::Expression,
                source,
                path,
                diagnostics,
            );
            walk_expr(
                else_branch,
                ParentExpr::Expression,
                source,
                path,
                diagnostics,
            );
        }
        Expr::Member(inner) => walk_expr(inner, ParentExpr::Member, source, path, diagnostics),
        Expr::Other { .. } => {}
    }
}

fn paren_is_unnecessary(inner: &Expr, parent: ParentExpr) -> bool {
    if complex(inner) {
        // Parentheses on the target of a member access, or inside another
        // expression, are the ones the pinned analyzer keeps.
        matches!(parent, ParentExpr::Other | ParentExpr::Assignment)
    } else {
        true
    }
}

fn complex(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Binary { .. } | Expr::Unary(_) | Expr::Cast(_) | Expr::Conditional { .. }
    )
}

fn report_arithmetic(
    child: &Expr,
    parent: Op,
    source: &Source,
    path: &str,
    diagnostics: &mut Vec<(Diagnostic, &'static str)>,
) {
    let Expr::Binary { op, left, .. } = child else {
        return;
    };
    if !matches!(op, Op::Add | Op::Mul | Op::Shift) || same_family(parent, *op) {
        return;
    }
    report_at(
        source,
        path,
        left.at(),
        "SA1407",
        "Arithmetic expressions should declare precedence",
        MAINTAINABILITY,
        diagnostics,
    );
}

fn report_conditional(
    child: &Expr,
    parent: Op,
    source: &Source,
    path: &str,
    diagnostics: &mut Vec<(Diagnostic, &'static str)>,
) {
    let Expr::Binary { op, left, .. } = child else {
        return;
    };
    if !matches!(op, Op::And | Op::Or) || *op == parent {
        return;
    }
    report_at(
        source,
        path,
        left.at(),
        "SA1408",
        "Conditional expressions should declare precedence",
        MAINTAINABILITY,
        diagnostics,
    );
}

fn same_family(left: Op, right: Op) -> bool {
    matches!(
        (left, right),
        (Op::Add, Op::Add) | (Op::Mul, Op::Mul) | (Op::Shift, Op::Shift)
    )
}

fn label_is_last(index: usize, statements: &[Stmt]) -> bool {
    statements[index + 1..]
        .iter()
        .all(|statement| statement.empty)
}

fn embedded_comments(
    source: &Source,
    tokens: &[Token],
    path: &str,
    parsed: &Parsed,
    diagnostics: &mut Vec<(Diagnostic, &'static str)>,
) {
    for (offset, code_index) in parsed.code.iter().enumerate() {
        if source.slice(tokens[*code_index].start, tokens[*code_index].end) != "{" {
            continue;
        }
        if !embedded_brace(source, tokens, &parsed.code, offset) {
            continue;
        }
        if offset > 0 {
            report_comments(
                source,
                path,
                &tokens[parsed.code[offset - 1]].trailing,
                diagnostics,
            );
        }
        report_comments(source, path, &tokens[*code_index].leading, diagnostics);
    }
}

fn embedded_brace(source: &Source, tokens: &[Token], code: &[usize], index: usize) -> bool {
    if index == 0 {
        return false;
    }
    let previous = text(source, tokens, code[index - 1]);
    if matches!(
        previous,
        "else" | "do" | "try" | "finally" | "checked" | "unchecked" | "catch"
    ) {
        return true;
    }
    if previous != ")" {
        return false;
    }
    let mut depth = 0i32;
    for cursor in (0..index).rev() {
        match text(source, tokens, code[cursor]) {
            ")" => depth += 1,
            "(" => {
                depth -= 1;
                if depth == 0 {
                    let keyword = cursor
                        .checked_sub(1)
                        .map(|index| text(source, tokens, code[index]));
                    return matches!(
                        keyword,
                        Some(
                            "if" | "for"
                                | "foreach"
                                | "while"
                                | "lock"
                                | "fixed"
                                | "switch"
                                | "catch"
                        )
                    );
                }
            }
            _ => {}
        }
    }
    false
}

fn report_comments(
    source: &Source,
    path: &str,
    trivia: &[crate::lex::Trivia],
    diagnostics: &mut Vec<(Diagnostic, &'static str)>,
) {
    for item in trivia {
        let text = source.slice(item.start, item.end);
        let comment = match item.kind {
            TriviaKind::SingleLineComment => !text.starts_with("////"),
            TriviaKind::MultiLineComment => true,
            _ => false,
        };
        if comment {
            report_at(
                source,
                path,
                item.start,
                "SA1108",
                "Block statements should not contain embedded comments",
                READABILITY,
                diagnostics,
            );
        }
    }
}

fn regions(
    source: &Source,
    tokens: &[Token],
    path: &str,
    parsed: &Parsed,
    diagnostics: &mut Vec<(Diagnostic, &'static str)>,
) {
    let mut regions = Vec::new();
    let mut trivia = Vec::new();
    crate::lex::flatten_trivia(tokens, &mut trivia);
    for item in &trivia {
        if item.kind != TriviaKind::Directive {
            continue;
        }
        let keyword = source.slice(item.detail_start, item.detail_end);
        if keyword == "region" {
            regions.push(item.start);
        } else if keyword == "endregion" {
            let Some(start) = regions.pop() else {
                continue;
            };
            let inside = outermost_block(parsed, start) == outermost_block(parsed, item.start)
                && outermost_block(parsed, start).is_some();
            if inside {
                report_at(
                    source,
                    path,
                    start,
                    "SA1123",
                    "Region should not be located within a code element",
                    READABILITY,
                    diagnostics,
                );
            } else {
                report_at(
                    source,
                    path,
                    start,
                    "SA1124",
                    "Do not use regions",
                    READABILITY,
                    diagnostics,
                );
            }
        }
    }
}

fn outermost_block(parsed: &Parsed, byte: u32) -> Option<usize> {
    parsed
        .block_spans
        .iter()
        .enumerate()
        .filter(|(_, (start, end))| *start <= byte && byte < *end)
        .min_by_key(|(_, (start, end))| end.saturating_sub(*start))
        .map(|(index, _)| index)
}

fn empty_strings(
    source: &Source,
    tokens: &[Token],
    path: &str,
    parsed: &Parsed,
    diagnostics: &mut Vec<(Diagnostic, &'static str)>,
) {
    for token in tokens {
        if token.kind != TokenKind::String {
            continue;
        }
        let text = source.slice(token.start, token.end);
        if text != "\"\"" && text != "@\"\"" {
            continue;
        }
        if string_must_be_constant(parsed, token.start) {
            continue;
        }
        report_at(
            source,
            path,
            token.start,
            "SA1122",
            "Use string.Empty for empty strings",
            READABILITY,
            diagnostics,
        );
    }
}

fn string_must_be_constant(parsed: &Parsed, byte: u32) -> bool {
    parsed
        .lists
        .iter()
        .flat_map(|list| &list.members)
        .any(|member| {
            member.is_const
                && member.kind == Kind::Field
                && byte >= member.start_at
                && byte <= member.name_at.saturating_add(256)
        })
}

fn constraints(
    source: &Source,
    tokens: &[Token],
    path: &str,
    parsed: &Parsed,
    diagnostics: &mut Vec<(Diagnostic, &'static str)>,
) {
    for (index, code_index) in parsed.code.iter().enumerate() {
        if text(source, tokens, *code_index) != "where" {
            continue;
        }
        let byte = tokens[*code_index].start;
        if first_in_line(source, byte) {
            continue;
        }
        let previous = index
            .checked_sub(1)
            .map(|previous| text(source, tokens, parsed.code[previous]));
        if !matches!(previous, Some(")" | ">")) {
            continue;
        }
        report_at(
            source,
            path,
            byte,
            "SA1127",
            "Generic type constraints should be on their own line",
            READABILITY,
            diagnostics,
        );
    }
}

fn constructor_initializers(
    source: &Source,
    path: &str,
    parsed: &Parsed,
    diagnostics: &mut Vec<(Diagnostic, &'static str)>,
) {
    for list in &parsed.lists {
        for member in &list.members {
            let Some(colon) = &member.colon else {
                continue;
            };
            if !colon.first_in_line || colon.trailing_newline {
                report_at(
                    source,
                    path,
                    colon.at,
                    "SA1128",
                    "Put constructor initializers on their own line",
                    READABILITY,
                    diagnostics,
                );
            }
        }
    }
}

fn combined_fields(
    source: &Source,
    path: &str,
    parsed: &Parsed,
    diagnostics: &mut Vec<(Diagnostic, &'static str)>,
) {
    for list in &parsed.lists {
        for member in &list.members {
            let field = member.kind == Kind::Field || member.event_field;
            if field && member.variables.len() >= 2 {
                report_at(
                    source,
                    path,
                    member.start_at,
                    "SA1132",
                    "Each field should be declared on its own line",
                    READABILITY,
                    diagnostics,
                );
            }
        }
    }
}

fn attributes(
    source: &Source,
    tokens: &[Token],
    path: &str,
    parsed: &Parsed,
    diagnostics: &mut Vec<(Diagnostic, &'static str)>,
) {
    let mut index = 0;
    let mut after_attribute = false;
    while index < parsed.code.len() {
        let token_text = text(source, tokens, parsed.code[index]);
        let starts_attribute = token_text == "["
            && (after_attribute || attribute_list(source, tokens, &parsed.code, index));
        if !starts_attribute {
            after_attribute = false;
            index += 1;
            continue;
        }
        after_attribute = true;
        let close = matching(source, tokens, &parsed.code, index, "[", "]");
        let open_byte = tokens[parsed.code[index]].start;
        let close_line = source
            .line_column(tokens[parsed.code[close.min(parsed.code.len() - 1)]].end as usize)
            .0;
        let mut names = Vec::new();
        let mut paren = 0i32;
        let mut cursor = index + 1;
        let mut name_ready = true;
        while cursor < close {
            let token_text = text(source, tokens, parsed.code[cursor]);
            match token_text {
                "(" => {
                    if paren == 0
                        && lookahead_text(source, tokens, &parsed.code, cursor + 1) == Some(")")
                    {
                        report_at(
                            source,
                            path,
                            tokens[parsed.code[cursor]].start,
                            "SA1411",
                            "Attribute constructor should not use unnecessary parenthesis",
                            MAINTAINABILITY,
                            diagnostics,
                        );
                    }
                    paren += 1;
                }
                ")" => paren -= 1,
                "," if paren == 0 => name_ready = true,
                _ if paren == 0 && name_ready && is_name(token_text) => {
                    names.push(tokens[parsed.code[cursor]].start);
                    name_ready = false;
                }
                _ => {}
            }
            cursor += 1;
        }
        if names.len() >= 2 {
            report_at(
                source,
                path,
                names[1],
                "SA1133",
                "Each attribute should be placed in its own set of square brackets",
                READABILITY,
                diagnostics,
            );
        }
        let previous_line = index.checked_sub(1).map(|previous| {
            source
                .line_column(tokens[parsed.code[previous]].end.saturating_sub(1) as usize)
                .0
        });
        let next_is_attribute =
            close + 1 < parsed.code.len() && text(source, tokens, parsed.code[close + 1]) == "[";
        let next_line = (close + 1 < parsed.code.len()).then(|| {
            source
                .line_column(tokens[parsed.code[close + 1]].end.saturating_sub(1) as usize)
                .0
        });
        if previous_line == Some(close_line)
            || (next_line == Some(close_line) && !next_is_attribute)
        {
            report_at(
                source,
                path,
                open_byte,
                "SA1134",
                "Each attribute should be placed on its own line of code",
                READABILITY,
                diagnostics,
            );
        }
        index = close + 1;
    }
}

fn attribute_list(source: &Source, tokens: &[Token], code: &[usize], index: usize) -> bool {
    if index == 0 {
        return true;
    }
    if !matches!(text(source, tokens, code[index - 1]), "{" | "}" | ";") {
        return false;
    }
    let close = matching(source, tokens, code, index, "[", "]");
    // `[key] = value` is an indexer initializer, not an attribute list.
    code.get(close + 1)
        .is_none_or(|token| text(source, tokens, *token) != "=")
}

fn enums(
    source: &Source,
    path: &str,
    parsed: &Parsed,
    diagnostics: &mut Vec<(Diagnostic, &'static str)>,
) {
    for body in &parsed.enums {
        let mut previous_line = None;
        for member in &body.members {
            if previous_line == Some(member.line) {
                report_at(
                    source,
                    path,
                    member.name_at,
                    "SA1136",
                    "Enum values should be on separate lines",
                    READABILITY,
                    diagnostics,
                );
            }
            previous_line = Some(member.line);
        }
        let start_line = source.line_column(body.start_at as usize).0;
        if !body.members.is_empty() && start_line != body.close_line && !body.trailing_comma {
            report_at(
                source,
                path,
                body.members
                    .last()
                    .map(|member| member.name_at)
                    .unwrap_or(body.start_at),
                "SA1413",
                "Use trailing comma in multi-line initializers",
                MAINTAINABILITY,
                diagnostics,
            );
        }
    }
}

fn initializers(
    source: &Source,
    path: &str,
    parsed: &Parsed,
    diagnostics: &mut Vec<(Diagnostic, &'static str)>,
) {
    for initializer in &parsed.initializers {
        if initializer.open_line != initializer.close_line && !initializer.trailing_comma {
            report_at(
                source,
                path,
                initializer.last_at,
                "SA1413",
                "Use trailing comma in multi-line initializers",
                MAINTAINABILITY,
                diagnostics,
            );
        }
    }
}

fn single_type(
    source: &Source,
    path: &str,
    parsed: &Parsed,
    diagnostics: &mut Vec<(Diagnostic, &'static str)>,
) {
    let types: Vec<_> = parsed
        .top_types
        .iter()
        .filter(|item| item.counts_as_class)
        .collect();
    let Some(first) = types.first() else {
        return;
    };
    let stem = file_stem(path);
    let preferred = types
        .iter()
        .find(|item| item.name == stem)
        .copied()
        .unwrap_or(*first);
    for item in types {
        if std::ptr::eq(item, preferred) {
            continue;
        }
        if preferred.partial && item.name == preferred.name {
            continue;
        }
        report_at(
            source,
            path,
            item.name_at,
            "SA1402",
            "File may only contain a single type",
            MAINTAINABILITY,
            diagnostics,
        );
    }
}

fn single_namespace(
    source: &Source,
    path: &str,
    parsed: &Parsed,
    diagnostics: &mut Vec<(Diagnostic, &'static str)>,
) {
    for byte in parsed.namespaces.iter().skip(1) {
        report_at(
            source,
            path,
            *byte,
            "SA1403",
            "File may only contain a single namespace",
            MAINTAINABILITY,
            diagnostics,
        );
    }
}

fn utf8(
    source: &Source,
    tokens: &[Token],
    path: &str,
    utf8_bom: bool,
    diagnostics: &mut Vec<(Diagnostic, &'static str)>,
) {
    if utf8_bom || crate::lex::whitespace_only(tokens) {
        return;
    }
    report_at(
        source,
        path,
        0,
        "SA1412",
        "Store files as UTF-8 with byte order mark",
        MAINTAINABILITY,
        diagnostics,
    );
}

fn file_stem(path: &str) -> &str {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.split_once('.').map(|(stem, _)| stem).unwrap_or(name)
}

fn text<'a>(source: &'a Source, tokens: &[Token], index: usize) -> &'a str {
    source.slice(tokens[index].start, tokens[index].end)
}

fn lookahead_text<'a>(
    source: &'a Source,
    tokens: &[Token],
    code: &[usize],
    index: usize,
) -> Option<&'a str> {
    code.get(index).map(|token| text(source, tokens, *token))
}

fn matching(
    source: &Source,
    tokens: &[Token],
    code: &[usize],
    start: usize,
    open: &str,
    close: &str,
) -> usize {
    let mut depth = 0i32;
    let mut index = start;
    while index < code.len() {
        let token = text(source, tokens, code[index]);
        if token == open {
            depth += 1;
        } else if token == close {
            depth -= 1;
            if depth == 0 {
                return index;
            }
        }
        index += 1;
    }
    code.len().saturating_sub(1)
}

fn is_name(text: &str) -> bool {
    text.starts_with(|ch: char| ch == '_' || ch == '@' || ch.is_ascii_alphabetic())
}

fn report_at(
    source: &Source,
    path: &str,
    byte: u32,
    code: &'static str,
    message: &str,
    category: &'static str,
    diagnostics: &mut Vec<(Diagnostic, &'static str)>,
) {
    let (line, column) = source.line_column(byte as usize);
    diagnostics.push((
        Diagnostic {
            path: path.to_owned(),
            line,
            column,
            code,
            message: message.to_owned(),
        },
        category,
    ));
}
