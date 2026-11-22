//! C# lexer that keeps trivia.
//!
//! Token boundaries follow Roslyn closely enough for whitespace rules: trivia stops
//! at the first newline, documentation comments stay structured, and inactive
//! preprocessor regions are disabled text rather than tokens.

use std::collections::HashSet;

use crate::source::{is_cs_newline, is_cs_whitespace};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Identifier,
    String,
    Char,
    InterpolatedText,
    Other,
    XmlText,
    XmlNewLine,
    EndOfFile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriviaKind {
    Whitespace,
    EndOfLine,
    SingleLineComment,
    MultiLineComment,
    DisabledText,
    DocComment,
    Directive,
    Exterior,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub start: u32,
    pub end: u32,
    pub leading: Vec<Trivia>,
    pub trailing: Vec<Trivia>,
    /// Open XML elements around a documentation text token, before this line's tags.
    pub doc_depth: i32,
}

impl Token {
    fn bare(kind: TokenKind, start: u32, end: u32) -> Self {
        Self {
            kind,
            start,
            end,
            leading: Vec::new(),
            trailing: Vec::new(),
            doc_depth: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Trivia {
    pub kind: TriviaKind,
    pub start: u32,
    pub end: u32,
    pub inner: Vec<Trivia>,
    pub tokens: Vec<Token>,
    /// Directive keyword span. Zero for other trivia.
    pub detail_start: u32,
    pub detail_end: u32,
}

impl Trivia {
    fn leaf(kind: TriviaKind, start: u32, end: u32) -> Self {
        Self {
            kind,
            start,
            end,
            inner: Vec::new(),
            tokens: Vec::new(),
            detail_start: 0,
            detail_end: 0,
        }
    }
}

#[derive(Clone, Debug)]
struct Branch {
    parent_active: bool,
    any_taken: bool,
    active: bool,
}

struct Lexer<'a> {
    text: &'a str,
    bytes: &'a [u8],
    pos: usize,
    at_line_start: bool,
    stack: Vec<Branch>,
    symbols: HashSet<String>,
}

impl<'a> Lexer<'a> {
    fn new(text: &'a str, symbols: HashSet<String>) -> Self {
        Self {
            text,
            bytes: text.as_bytes(),
            pos: 0,
            at_line_start: true,
            stack: Vec::new(),
            symbols,
        }
    }

    fn eof(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn peek(&self) -> Option<char> {
        self.text[self.pos..].chars().next()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.text[self.pos..].chars().nth(offset)
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn active(&self) -> bool {
        self.stack.iter().all(|branch| branch.active)
    }

    fn mark_content(&mut self) {
        self.at_line_start = false;
    }

    fn scan_tokens(mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let leading = self.scan_trivia(true);
            if self.eof() {
                tokens.push(Token {
                    kind: TokenKind::EndOfFile,
                    start: self.pos as u32,
                    end: self.pos as u32,
                    leading,
                    trailing: Vec::new(),
                    doc_depth: 0,
                });
                break;
            }
            let mut parts = self.scan_token_parts();
            let trailing = self.scan_trivia(false);
            if parts.is_empty() {
                continue;
            }
            let last = parts.len() - 1;
            parts[0].leading = leading;
            parts[last].trailing = trailing;
            tokens.append(&mut parts);
        }
        tokens
    }

    fn scan_trivia(&mut self, leading: bool) -> Vec<Trivia> {
        let mut trivia = Vec::new();
        loop {
            if !self.active() {
                if self.at_line_start && self.peek() == Some('#') {
                    trivia.push(self.scan_directive());
                    continue;
                }
                let disabled = self.scan_disabled();
                if disabled.end > disabled.start {
                    trivia.push(disabled);
                }
                if self.eof() || !(self.at_line_start && self.peek() == Some('#')) {
                    break;
                }
                continue;
            }
            if self.at_line_start && self.peek() == Some('#') {
                if !leading && trivia.iter().any(|item| item.kind == TriviaKind::EndOfLine) {
                    break;
                }
                // A directive after a newline belongs to the next token's leading trivia.
                if !leading {
                    break;
                }
                trivia.push(self.scan_directive());
                continue;
            }
            if self.is_doc_comment() {
                trivia.push(self.scan_doc_comment());
                continue;
            }
            if self.starts_line_comment() {
                trivia.push(self.scan_line_comment());
                continue;
            }
            if self.starts_block_comment() {
                trivia.push(self.scan_block_comment());
                continue;
            }
            if self.peek().is_some_and(is_cs_whitespace) {
                trivia.push(self.scan_whitespace());
                continue;
            }
            if self.peek().is_some_and(is_cs_newline) {
                trivia.push(self.scan_newline());
                if !leading {
                    break;
                }
                continue;
            }
            break;
        }
        trivia
    }

    fn scan_disabled(&mut self) -> Trivia {
        let start = self.pos as u32;
        while !self.eof() {
            if self.at_line_start && self.peek() == Some('#') {
                break;
            }
            if self.peek().is_some_and(is_cs_newline) {
                self.consume_newline();
                self.at_line_start = true;
                continue;
            }
            let ch = self.bump().unwrap();
            if !is_cs_whitespace(ch) {
                self.mark_content();
            }
        }
        Trivia::leaf(TriviaKind::DisabledText, start, self.pos as u32)
    }

    fn scan_whitespace(&mut self) -> Trivia {
        let start = self.pos as u32;
        while self.peek().is_some_and(is_cs_whitespace) {
            self.bump();
        }
        Trivia::leaf(TriviaKind::Whitespace, start, self.pos as u32)
    }

    fn scan_newline(&mut self) -> Trivia {
        let start = self.pos as u32;
        self.consume_newline();
        self.at_line_start = true;
        Trivia::leaf(TriviaKind::EndOfLine, start, self.pos as u32)
    }

    fn consume_newline(&mut self) {
        if self.peek() == Some('\r') {
            self.bump();
            if self.peek() == Some('\n') {
                self.bump();
            }
        } else if self.peek().is_some_and(is_cs_newline) {
            self.bump();
        }
    }

    fn scan_line_comment(&mut self) -> Trivia {
        let start = self.pos as u32;
        self.bump();
        self.bump();
        while !self.eof() && !self.peek().is_some_and(is_cs_newline) {
            self.bump();
        }
        self.mark_content();
        Trivia::leaf(TriviaKind::SingleLineComment, start, self.pos as u32)
    }

    fn scan_block_comment(&mut self) -> Trivia {
        let start = self.pos as u32;
        self.bump();
        self.bump();
        self.mark_content();
        while !self.eof() {
            if self.peek() == Some('*') && self.peek_at(1) == Some('/') {
                self.bump();
                self.bump();
                break;
            }
            if self.peek().is_some_and(is_cs_newline) {
                self.consume_newline();
                self.at_line_start = true;
            } else {
                self.bump();
                self.mark_content();
            }
        }
        Trivia::leaf(TriviaKind::MultiLineComment, start, self.pos as u32)
    }

    fn starts_line_comment(&self) -> bool {
        self.peek() == Some('/') && self.peek_at(1) == Some('/')
    }

    fn starts_block_comment(&self) -> bool {
        self.peek() == Some('/') && self.peek_at(1) == Some('*')
    }

    fn is_doc_comment(&self) -> bool {
        self.at_line_start
            && self.starts_line_comment()
            && self.peek_at(2) == Some('/')
            && self.peek_at(3) != Some('/')
    }

    fn scan_doc_comment(&mut self) -> Trivia {
        let start = self.pos as u32;
        let mut tokens = Vec::new();
        let mut depth = 0;
        loop {
            let exterior_start = self.pos;
            // Continuation lines fold their indentation into the exterior.
            if self.at_line_start {
                while self.peek().is_some_and(is_cs_whitespace) {
                    self.bump();
                }
            }
            self.bump();
            self.bump();
            self.bump();
            let exterior =
                Trivia::leaf(TriviaKind::Exterior, exterior_start as u32, self.pos as u32);
            depth = self.scan_doc_line(exterior, depth, &mut tokens);
            if self.peek().is_some_and(is_cs_newline) {
                let nl_start = self.pos as u32;
                self.consume_newline();
                self.at_line_start = true;
                tokens.push(Token::bare(
                    TokenKind::XmlNewLine,
                    nl_start,
                    self.pos as u32,
                ));
            } else {
                break;
            }
            if !self.lookahead_doc_line() {
                break;
            }
        }
        let mut trivia = Trivia::leaf(TriviaKind::DocComment, start, self.pos as u32);
        trivia.tokens = tokens;
        trivia
    }

    fn lookahead_doc_line(&self) -> bool {
        let mut rest = self.text[self.pos..].chars();
        loop {
            match rest.next() {
                Some(ch) if is_cs_whitespace(ch) => continue,
                Some('/') => {
                    return rest.next() == Some('/')
                        && rest.next() == Some('/')
                        && rest.next() != Some('/');
                }
                _ => return false,
            }
        }
    }

    fn scan_doc_line(
        &mut self,
        exterior: Trivia,
        depth_at_line: i32,
        tokens: &mut Vec<Token>,
    ) -> i32 {
        let mut depth = depth_at_line;
        let mut first = true;
        let attached = exterior;
        while !self.eof() && !self.peek().is_some_and(is_cs_newline) {
            if self.peek() == Some('<') {
                let start = self.pos as u32;
                depth = self.scan_xml_tag(depth);
                let mut token = Token::bare(TokenKind::Other, start, self.pos as u32);
                token.doc_depth = depth_at_line;
                if first {
                    token.leading.push(attached.clone());
                    first = false;
                }
                tokens.push(token);
            } else {
                let start = self.pos as u32;
                while !self.eof()
                    && !self.peek().is_some_and(is_cs_newline)
                    && self.peek() != Some('<')
                {
                    self.bump();
                }
                let mut token = Token::bare(TokenKind::XmlText, start, self.pos as u32);
                token.doc_depth = depth_at_line;
                if first {
                    token.leading.push(attached.clone());
                    first = false;
                }
                tokens.push(token);
            }
        }
        if first {
            let mut token = Token::bare(TokenKind::XmlText, self.pos as u32, self.pos as u32);
            token.doc_depth = depth_at_line;
            token.leading.push(attached);
            tokens.push(token);
        }
        depth
    }

    fn scan_xml_tag(&mut self, depth: i32) -> i32 {
        self.bump();
        let closing = self.peek() == Some('/');
        if closing {
            self.bump();
        }
        let mut self_close = false;
        while !self.eof() && !self.peek().is_some_and(is_cs_newline) && self.peek() != Some('>') {
            if self.peek() == Some('/') && self.peek_at(1) == Some('>') {
                self_close = true;
            }
            self.bump();
        }
        if self.peek() == Some('>') {
            self.bump();
        }
        if closing {
            depth - 1
        } else if self_close {
            depth
        } else {
            depth + 1
        }
    }

    fn scan_directive(&mut self) -> Trivia {
        let start = self.pos as u32;
        self.bump();
        self.mark_content();
        let mut inner = Vec::new();
        if self.peek().is_some_and(is_cs_whitespace) {
            inner.push(self.scan_whitespace());
        }
        let keyword_start = self.pos as u32;
        let keyword = self.scan_raw_ident();
        let keyword_end = self.pos as u32;
        let rest_start = self.pos;
        while !self.eof() && !self.peek().is_some_and(is_cs_newline) {
            if self.peek().is_some_and(is_cs_whitespace) {
                inner.push(self.scan_whitespace());
            } else {
                self.bump();
            }
        }
        let content_end = self.pos as u32;
        if self.peek().is_some_and(is_cs_newline) {
            inner.push(self.scan_newline());
        }
        self.apply_directive(&keyword, rest_start, content_end as usize);
        let mut trivia = Trivia::leaf(TriviaKind::Directive, start, content_end);
        trivia.inner = inner;
        trivia.detail_start = keyword_start;
        trivia.detail_end = keyword_end;
        trivia
    }

    fn scan_raw_ident(&mut self) -> String {
        let start = self.pos;
        if self.peek() == Some('@') {
            self.bump();
        }
        while self.peek().is_some_and(is_ident_continue) {
            self.bump();
        }
        self.text[start..self.pos].to_string()
    }

    fn apply_directive(&mut self, keyword: &str, rest_start: usize, rest_end: usize) {
        let rest = &self.text[rest_start..rest_end];
        match keyword {
            "if" => {
                let parent_active = self.active();
                let taken = parent_active && eval_pp(rest, &self.symbols);
                self.stack.push(Branch {
                    parent_active,
                    any_taken: taken,
                    active: taken,
                });
            }
            "elif" => {
                let cond = eval_pp(rest, &self.symbols);
                if let Some(branch) = self.stack.last_mut() {
                    if branch.parent_active && !branch.any_taken && cond {
                        branch.any_taken = true;
                        branch.active = true;
                    } else {
                        branch.active = false;
                    }
                }
            }
            "else" => {
                if let Some(branch) = self.stack.last_mut() {
                    if branch.parent_active && !branch.any_taken {
                        branch.any_taken = true;
                        branch.active = true;
                    } else {
                        branch.active = false;
                    }
                }
            }
            "endif" => {
                self.stack.pop();
            }
            "define" if self.active() => {
                if let Some(name) = first_ident(rest) {
                    self.symbols.insert(name);
                }
            }
            "undef" if self.active() => {
                if let Some(name) = first_ident(rest) {
                    self.symbols.remove(&name);
                }
            }
            _ => {}
        }
    }

    fn scan_token_parts(&mut self) -> Vec<Token> {
        let start = self.pos as u32;
        self.mark_content();
        if self.starts_interpolated() {
            return self.scan_interpolated();
        }
        if self.peek() == Some('@') && self.peek_at(1) == Some('"') {
            self.scan_verbatim_string();
            return vec![Token::bare(TokenKind::String, start, self.pos as u32)];
        }
        if self.peek() == Some('@') {
            self.scan_raw_ident();
            return vec![Token::bare(TokenKind::Identifier, start, self.pos as u32)];
        }
        if self.peek() == Some('"') {
            if self.peek_at(1) == Some('"') && self.peek_at(2) == Some('"') {
                self.scan_raw_string();
            } else {
                self.scan_regular_string(false);
            }
            return vec![Token::bare(TokenKind::String, start, self.pos as u32)];
        }
        if self.peek() == Some('\'') {
            self.scan_char();
            return vec![Token::bare(TokenKind::Char, start, self.pos as u32)];
        }
        if self.peek().is_some_and(is_ident_start) {
            self.scan_raw_ident();
            return vec![Token::bare(TokenKind::Identifier, start, self.pos as u32)];
        }
        if self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
            self.scan_number();
            return vec![Token::bare(TokenKind::Other, start, self.pos as u32)];
        }
        let width = self.operator_width();
        for _ in 0..width {
            self.bump();
        }
        vec![Token::bare(TokenKind::Other, start, self.pos as u32)]
    }

    fn operator_width(&self) -> usize {
        match (self.peek(), self.peek_at(1), self.peek_at(2)) {
            (Some('<'), Some('<'), Some('='))
            | (Some('>'), Some('>'), Some('='))
            | (Some('?'), Some('?'), Some('='))
            | (Some('.'), Some('.'), Some('=')) => 3,
            (Some('='), Some('>' | '='), _)
            | (Some('!' | '<' | '>'), Some('='), _)
            | (Some('&'), Some('&' | '='), _)
            | (Some('|'), Some('|' | '='), _)
            | (Some('+'), Some('+' | '='), _)
            | (Some('-'), Some('-' | '=' | '>'), _)
            | (Some('*' | '/' | '%' | '^'), Some('='), _)
            | (Some('?'), Some('?' | '.'), _)
            | (Some(':'), Some(':'), _)
            | (Some('.'), Some('.'), _)
            | (Some('<'), Some('<'), _) => 2,
            _ => 1,
        }
    }

    fn scan_regular_string(&mut self, stop_at_newline: bool) {
        self.bump();
        while !self.eof() {
            if stop_at_newline && self.peek().is_some_and(is_cs_newline) {
                break;
            }
            if self.peek() == Some('\\') {
                self.bump();
                if !self.eof() {
                    self.bump();
                }
                continue;
            }
            if self.peek() == Some('"') {
                self.bump();
                break;
            }
            if self.peek().is_some_and(is_cs_newline) {
                break;
            }
            self.bump();
        }
    }

    fn scan_verbatim_string(&mut self) {
        self.bump();
        self.bump();
        while !self.eof() {
            if self.peek() == Some('"') {
                self.bump();
                if self.peek() == Some('"') {
                    self.bump();
                    continue;
                }
                break;
            }
            if self.peek().is_some_and(is_cs_newline) {
                self.consume_newline();
                self.at_line_start = true;
            } else {
                self.bump();
            }
        }
    }

    fn scan_raw_string(&mut self) {
        let mut quotes = 0;
        while self.peek() == Some('"') {
            self.bump();
            quotes += 1;
        }
        while !self.eof() {
            if self.peek() == Some('"') {
                let mut count = 0;
                while self.peek_at(count) == Some('"') {
                    count += 1;
                }
                if count >= quotes {
                    for _ in 0..quotes {
                        self.bump();
                    }
                    break;
                }
            }
            if self.peek().is_some_and(is_cs_newline) {
                self.consume_newline();
                self.at_line_start = true;
            } else {
                self.bump();
            }
        }
    }

    fn scan_char(&mut self) {
        self.bump();
        while !self.eof() && !self.peek().is_some_and(is_cs_newline) {
            if self.peek() == Some('\\') {
                self.bump();
                if !self.eof() {
                    self.bump();
                }
                continue;
            }
            if self.peek() == Some('\'') {
                self.bump();
                break;
            }
            self.bump();
        }
    }

    fn scan_number(&mut self) {
        if self.peek() == Some('0') && matches!(self.peek_at(1), Some('x' | 'X' | 'b' | 'B')) {
            self.bump();
            self.bump();
        }
        while self
            .peek()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.')
        {
            self.bump();
        }
    }

    fn starts_interpolated(&self) -> bool {
        matches!(
            (self.peek(), self.peek_at(1), self.peek_at(2)),
            (Some('$'), Some('"'), _)
                | (Some('$'), Some('@'), Some('"'))
                | (Some('@'), Some('$'), Some('"'))
        )
    }

    fn scan_interpolated(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        let start = self.pos as u32;
        let mut verbatim = false;
        while matches!(self.peek(), Some('@' | '$')) {
            if self.peek() == Some('@') {
                verbatim = true;
            }
            self.bump();
        }
        if self.peek() == Some('"') {
            self.bump();
        }
        tokens.push(Token::bare(TokenKind::Other, start, self.pos as u32));
        self.scan_interp_content(&mut tokens, verbatim);
        tokens
    }

    fn scan_interp_content(&mut self, tokens: &mut Vec<Token>, verbatim: bool) {
        let mut text_start = self.pos;
        while !self.eof() {
            if !verbatim && self.peek().is_some_and(is_cs_newline) {
                break;
            }
            if !verbatim && self.peek() == Some('\\') {
                self.bump();
                if !self.eof() {
                    self.bump();
                }
                continue;
            }
            if verbatim && self.peek() == Some('"') && self.peek_at(1) == Some('"') {
                self.bump();
                self.bump();
                continue;
            }
            if self.peek() == Some('"') {
                self.flush_interp_text(tokens, text_start);
                let end_start = self.pos as u32;
                self.bump();
                tokens.push(Token::bare(TokenKind::Other, end_start, self.pos as u32));
                return;
            }
            if self.peek() == Some('{') && self.peek_at(1) == Some('{') {
                self.bump();
                self.bump();
                continue;
            }
            if self.peek() == Some('}') && self.peek_at(1) == Some('}') {
                self.bump();
                self.bump();
                continue;
            }
            if self.peek() == Some('{') {
                self.flush_interp_text(tokens, text_start);
                let brace = self.pos as u32;
                self.bump();
                tokens.push(Token::bare(TokenKind::Other, brace, self.pos as u32));
                self.scan_interp_hole(tokens, verbatim);
                text_start = self.pos;
                continue;
            }
            if verbatim && self.peek().is_some_and(is_cs_newline) {
                self.consume_newline();
                self.at_line_start = true;
                continue;
            }
            self.bump();
        }
        self.flush_interp_text(tokens, text_start);
    }

    fn flush_interp_text(&self, tokens: &mut Vec<Token>, text_start: usize) {
        if self.pos > text_start {
            tokens.push(Token::bare(
                TokenKind::InterpolatedText,
                text_start as u32,
                self.pos as u32,
            ));
        }
    }

    fn scan_interp_hole(&mut self, tokens: &mut Vec<Token>, verbatim: bool) {
        let mut depth = 1;
        let mut nested = 0usize;
        while !self.eof() && depth > 0 {
            if self.peek() == Some(':') && self.peek_at(1) == Some(':') {
                tokens.extend(self.scan_token_parts());
                continue;
            }
            if depth == 1 && nested == 0 && self.peek() == Some(':') {
                self.scan_interp_format(tokens, verbatim);
                if self.peek() != Some('}') {
                    break;
                }
            }
            if self.peek() == Some('{') {
                depth += 1;
                let start = self.pos as u32;
                self.bump();
                tokens.push(Token::bare(TokenKind::Other, start, self.pos as u32));
                continue;
            }
            if self.peek() == Some('}') {
                depth -= 1;
                let start = self.pos as u32;
                self.bump();
                tokens.push(Token::bare(TokenKind::Other, start, self.pos as u32));
                continue;
            }
            if self.peek() == Some('"')
                || self.peek() == Some('\'')
                || self.peek() == Some('@')
                || self.peek() == Some('$')
            {
                tokens.extend(self.scan_token_parts());
                continue;
            }
            match self.peek() {
                Some('(' | '[') => nested += 1,
                Some(')' | ']') => nested = nested.saturating_sub(1),
                _ => {}
            }
            let start = self.pos as u32;
            self.bump();
            tokens.push(Token::bare(TokenKind::Other, start, self.pos as u32));
        }
    }

    fn scan_interp_format(&mut self, tokens: &mut Vec<Token>, verbatim: bool) {
        let start = self.pos;
        while let Some(character) = self.peek() {
            match character {
                '}' => break,
                '"' if verbatim && self.peek_at(1) == Some('"') => {
                    self.bump();
                    self.bump();
                }
                '"' => break,
                '\\' if !verbatim => {
                    self.bump();
                    self.bump();
                }
                _ if !verbatim && is_cs_newline(character) => break,
                _ => {
                    self.bump();
                }
            }
        }
        self.flush_interp_text(tokens, start);
    }
}

fn is_ident_start(ch: char) -> bool {
    ch == '_' || ch.is_alphabetic()
}

fn is_ident_continue(ch: char) -> bool {
    ch == '_' || ch.is_alphanumeric()
}

fn first_ident(text: &str) -> Option<String> {
    let mut chars = text.chars().peekable();
    while chars.peek().is_some_and(|ch| is_cs_whitespace(*ch)) {
        chars.next();
    }
    let mut name = String::new();
    if chars.peek().is_some_and(|ch| is_ident_start(*ch)) {
        name.push(chars.next().unwrap());
        while chars.peek().is_some_and(|ch| is_ident_continue(*ch)) {
            name.push(chars.next().unwrap());
        }
    }
    if name.is_empty() { None } else { Some(name) }
}

fn eval_pp(text: &str, symbols: &HashSet<String>) -> bool {
    let mut parser = PpParser {
        chars: text.chars().collect(),
        index: 0,
        symbols,
    };
    parser.parse_or()
}

struct PpParser<'a> {
    chars: Vec<char>,
    index: usize,
    symbols: &'a HashSet<String>,
}

impl<'a> PpParser<'a> {
    fn parse_or(&mut self) -> bool {
        let mut value = self.parse_and();
        while self.eat_op("||") {
            let right = self.parse_and();
            value = value || right;
        }
        value
    }

    fn parse_and(&mut self) -> bool {
        let mut value = self.parse_eq();
        while self.eat_op("&&") {
            let right = self.parse_eq();
            value = value && right;
        }
        value
    }

    fn parse_eq(&mut self) -> bool {
        let mut value = self.parse_unary();
        loop {
            if self.eat_op("==") {
                value = value == self.parse_unary();
            } else if self.eat_op("!=") {
                value = value != self.parse_unary();
            } else {
                break;
            }
        }
        value
    }

    fn parse_unary(&mut self) -> bool {
        self.skip();
        if self.eat_op("!") {
            return !self.parse_unary();
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> bool {
        self.skip();
        if self.eat_op("(") {
            let value = self.parse_or();
            self.eat_op(")");
            return value;
        }
        let word = self.word();
        match word.as_str() {
            "true" => true,
            "false" => false,
            "" => false,
            other => self.symbols.contains(other),
        }
    }

    fn word(&mut self) -> String {
        self.skip();
        let mut name = String::new();
        if self.index < self.chars.len() && is_ident_start(self.chars[self.index]) {
            name.push(self.chars[self.index]);
            self.index += 1;
            while self.index < self.chars.len() && is_ident_continue(self.chars[self.index]) {
                name.push(self.chars[self.index]);
                self.index += 1;
            }
        }
        name
    }

    fn eat_op(&mut self, op: &str) -> bool {
        self.skip();
        let rest: String = self.chars[self.index..].iter().collect();
        if rest.starts_with(op) {
            self.index += op.chars().count();
            true
        } else {
            false
        }
    }

    fn skip(&mut self) {
        while self.index < self.chars.len() && is_cs_whitespace(self.chars[self.index]) {
            self.index += 1;
        }
    }
}

pub fn default_symbols() -> HashSet<String> {
    [
        "TRACE",
        "DEBUG",
        "NET",
        "NET10_0",
        "NETCOREAPP",
        "NET5_0_OR_GREATER",
        "NET6_0_OR_GREATER",
        "NET7_0_OR_GREATER",
        "NET8_0_OR_GREATER",
        "NET9_0_OR_GREATER",
        "NET10_0_OR_GREATER",
        "NETCOREAPP1_0_OR_GREATER",
        "NETCOREAPP1_1_OR_GREATER",
        "NETCOREAPP2_0_OR_GREATER",
        "NETCOREAPP2_1_OR_GREATER",
        "NETCOREAPP2_2_OR_GREATER",
        "NETCOREAPP3_0_OR_GREATER",
        "NETCOREAPP3_1_OR_GREATER",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

pub fn lex(text: &str) -> Vec<Token> {
    Lexer::new(text, default_symbols()).scan_tokens()
}

pub fn flatten_trivia<'a>(tokens: &'a [Token], out: &mut Vec<&'a Trivia>) {
    for token in tokens {
        for trivia in &token.leading {
            flatten_one(trivia, out);
        }
        for trivia in &token.trailing {
            flatten_one(trivia, out);
        }
    }
}

fn flatten_one<'a>(trivia: &'a Trivia, out: &mut Vec<&'a Trivia>) {
    out.push(trivia);
    for inner in &trivia.inner {
        flatten_one(inner, out);
    }
    for token in &trivia.tokens {
        for leading in &token.leading {
            flatten_one(leading, out);
        }
        for trailing in &token.trailing {
            flatten_one(trailing, out);
        }
    }
}

pub fn whitespace_only(tokens: &[Token]) -> bool {
    match tokens {
        [token] if token.kind == TokenKind::EndOfFile => token
            .leading
            .iter()
            .all(|trivia| matches!(trivia.kind, TriviaKind::Whitespace | TriviaKind::EndOfLine)),
        _ => false,
    }
}
