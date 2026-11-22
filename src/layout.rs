//! Layout rules SA1500–SA1520 that can be decided from tokens and blank lines.
//!
//! SA1518 defaults to allow: a missing newline is fine, and so is one trailing
//! newline. `insert_final_newline` switches that to require or omit.

use std::collections::HashSet;

use crate::config::{FileSettings, NewlineAtEnd};
use crate::lex::{Token, TokenKind, Trivia, TriviaKind, whitespace_only};
use crate::source::Source;
use crate::whitespace::Diagnostic;

const LAYOUT: &str = "stylecop.csharp.layoutrules";

pub fn layout_diagnostics(
    source: &Source,
    tokens: &[Token],
    path: &str,
    settings: &FileSettings,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if whitespace_only(tokens) {
        return diagnostics;
    }
    blank_lines(source, tokens, path, &mut diagnostics);
    walk(source, tokens, path, settings, &mut diagnostics);
    diagnostics.retain(|diagnostic| settings.allows(diagnostic.code, LAYOUT));
    diagnostics
}

fn blank_lines(source: &Source, tokens: &[Token], path: &str, diagnostics: &mut Vec<Diagnostic>) {
    let Some(first) = tokens.first() else {
        return;
    };
    if let Some(span) = leading_blank_span(&first.leading) {
        report(
            source,
            path,
            span,
            "SA1517",
            "Code should not contain blank lines at start of file",
            diagnostics,
        );
    }
    for token in tokens {
        if token.kind == TokenKind::EndOfFile {
            continue;
        }
        if let Some(span) = multiple_blank_span(&token.leading) {
            report(
                source,
                path,
                span,
                "SA1507",
                "Code should not contain multiple blank lines in a row",
                diagnostics,
            );
        }
    }
}

fn leading_blank_span(trivia: &[Trivia]) -> Option<u32> {
    let mut newlines = 0;
    for item in trivia {
        match item.kind {
            TriviaKind::Whitespace => {}
            TriviaKind::EndOfLine => newlines += 1,
            _ => {
                return if newlines > 0 {
                    Some(trivia[0].start)
                } else {
                    None
                };
            }
        }
    }
    if newlines > 0 {
        Some(trivia.first()?.start)
    } else {
        None
    }
}

fn multiple_blank_span(trivia: &[Trivia]) -> Option<u32> {
    let mut start: Option<u32> = None;
    let mut count = 0;
    let mut after_comment = false;
    for item in trivia {
        match item.kind {
            TriviaKind::Whitespace => {}
            TriviaKind::EndOfLine => {
                if start.is_none() {
                    start = Some(item.start);
                }
                count += 1;
            }
            TriviaKind::SingleLineComment | TriviaKind::MultiLineComment => {
                start = None;
                count = 0;
                after_comment = true;
            }
            _ => {
                if after_comment && count > 0 {
                    count -= 1;
                }
                if count >= 2 && item.start != 0 {
                    return start;
                }
                start = None;
                count = 0;
                after_comment = false;
            }
        }
    }
    if after_comment && count > 0 {
        count -= 1;
    }
    if count >= 2 { start } else { None }
}

fn walk(
    source: &Source,
    tokens: &[Token],
    path: &str,
    settings: &FileSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut pass = Pass {
        source,
        tokens,
        path,
        holes: hole_braces(source, tokens),
        diagnostics,
        levels: vec![Level::root()],
        paren: 0,
        bracket: 0,
        next_container: 1,
        members: Vec::new(),
    };
    pass.scan();
    pass.statements();
    pass.chained_blanks();
    pass.comments();
    pass.documentation();
    pass.separate_members();
    pass.end_of_file(settings.newline_at_end);
}

struct Pass<'a> {
    source: &'a Source,
    tokens: &'a [Token],
    path: &'a str,
    holes: HashSet<usize>,
    diagnostics: &'a mut Vec<Diagnostic>,
    levels: Vec<Level>,
    paren: i32,
    bracket: i32,
    next_container: usize,
    members: Vec<Member>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Root,
    Namespace,
    Type,
    Enum,
    Method,
    AccessorList,
    Accessor,
    Statement,
    Lambda,
    Initializer,
    Other,
}

struct Level {
    kind: Kind,
    open: usize,
    container: usize,
    member: Option<Partial>,
    accessors: Vec<Accessor>,
}

impl Level {
    fn root() -> Self {
        Self {
            kind: Kind::Root,
            open: 0,
            container: 0,
            member: None,
            accessors: Vec::new(),
        }
    }
}

struct Partial {
    start: usize,
    saw_parameter_list: bool,
    generic_depth: i32,
    saw_event: bool,
    saw_equals: bool,
    saw_arrow: bool,
    /// The accessor list closed, and an initializer (`=` / `=>`) follows it.
    continued: bool,
    body: Option<usize>,
}

struct Accessor {
    keyword: usize,
    end: Option<usize>,
    has_body: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MemberKind {
    Field,
    Using,
    Other,
    Invalid,
}

struct Member {
    container: usize,
    start: usize,
    end: usize,
    kind: MemberKind,
    single: bool,
    has_doc: bool,
}

impl<'a> Pass<'a> {
    fn scan(&mut self) {
        for index in 0..self.tokens.len() {
            if self.tokens[index].kind == TokenKind::EndOfFile {
                break;
            }
            let open = self.counts_as_code(index)
                && self.text(index) == "{"
                && !self.holes.contains(&index);
            let close = self.counts_as_code(index)
                && self.text(index) == "}"
                && !self.holes.contains(&index);
            if open {
                self.open_brace(index);
            } else if close {
                self.close_brace(index);
            } else {
                self.on_token(index);
            }
            self.shift_depth(index);
        }
    }

    fn open_brace(&mut self, index: usize) {
        if self.paren > 0 {
            let kind = if self.prev(index).is_some_and(|prev| self.text(prev) == "=>") {
                Kind::Lambda
            } else {
                Kind::Other
            };
            let container = self.levels.last().map(|level| level.container).unwrap_or(0);
            self.levels.push(Level {
                kind,
                open: index,
                container,
                member: None,
                accessors: Vec::new(),
            });
            return;
        }
        let parent = self
            .levels
            .last()
            .map(|level| level.kind)
            .unwrap_or(Kind::Root);
        let kind = self.classify(index, parent);
        if self.preceded_by_blank_gap(index) && self.text(self.prev(index).unwrap_or(index)) != "}"
        {
            self.emit(
                self.tokens[index].start,
                "SA1509",
                "Opening braces should not be preceded by blank line",
            );
        }
        if self.followed_by_blank_gap(index) {
            self.emit(
                self.tokens[index].start,
                "SA1505",
                "An opening brace should not be followed by a blank line",
            );
        }
        if is_member_body(kind)
            && self
                .levels
                .last()
                .is_some_and(|level| is_container(level.kind))
            && let Some(member) = self
                .levels
                .last_mut()
                .and_then(|level| level.member.as_mut())
            && member.body.is_none()
        {
            member.body = Some(index);
        }
        if parent == Kind::AccessorList
            && kind == Kind::Accessor
            && let Some(accessor) = self
                .levels
                .last_mut()
                .and_then(|level| level.accessors.last_mut())
            && accessor.end.is_none()
        {
            accessor.has_body = true;
        }
        let container = if matches!(kind, Kind::Type | Kind::Namespace | Kind::Enum) {
            let id = self.next_container;
            self.next_container += 1;
            id
        } else {
            self.levels.last().map(|level| level.container).unwrap_or(0)
        };
        self.levels.push(Level {
            kind,
            open: index,
            container,
            member: None,
            accessors: Vec::new(),
        });
    }

    fn close_brace(&mut self, index: usize) {
        if self.levels.len() <= 1 {
            return;
        }
        let finished = self.levels.pop().expect("brace");
        self.finish_brace(finished.kind, finished.open, index);
        if finished.kind == Kind::Accessor
            && let Some(accessor) = self
                .levels
                .last_mut()
                .and_then(|level| level.accessors.last_mut())
            && accessor.end.is_none()
        {
            accessor.end = Some(index);
        }
        if finished.kind == Kind::AccessorList {
            self.finish_accessors(&finished.accessors);
        }
        let closes_member = self
            .levels
            .last()
            .and_then(|level| level.member.as_ref())
            .and_then(|member| member.body)
            == Some(finished.open);
        if closes_member {
            let next_text = self.next(index).map(|next| self.text(next).to_string());
            let keeps_going = next_text
                .as_deref()
                .is_some_and(|text| text == "=" || text == "=>");
            if keeps_going {
                if let Some(member) = self
                    .levels
                    .last_mut()
                    .and_then(|level| level.member.as_mut())
                {
                    member.body = None;
                    member.continued = true;
                    member.saw_arrow = next_text.as_deref() == Some("=>");
                }
            } else {
                let container = self.levels.last().map(|level| level.container).unwrap_or(0);
                let member = self.levels.last_mut().and_then(|level| level.member.take());
                if let Some(member) = member {
                    self.finish_member(container, member, index);
                }
            }
        }
    }

    fn on_token(&mut self, index: usize) {
        let text = self.text(index).to_string();
        let container = self
            .levels
            .last()
            .is_some_and(|level| is_container(level.kind));
        let at_level = container && self.paren == 0 && self.bracket == 0;
        if at_level
            && self
                .levels
                .last()
                .is_some_and(|level| level.member.is_none())
        {
            if std::env::var_os("SC_DEBUG").is_some() {
                eprintln!(
                    "start L{} {} {}",
                    self.line(index),
                    self.source.line_column(self.tokens[index].start as usize).1,
                    text
                );
            }
            self.levels.last_mut().unwrap().member = Some(Partial {
                start: index,
                saw_parameter_list: false,
                generic_depth: 0,
                saw_event: false,
                saw_equals: false,
                saw_arrow: false,
                continued: false,
                body: None,
            });
        }
        if std::env::var_os("SC_DEBUG").is_some() && (text == ";" || text == "{") {
            let top = self
                .levels
                .last()
                .map(|level| level.kind as u8)
                .unwrap_or(0);
            let body = self
                .levels
                .last()
                .and_then(|level| level.member.as_ref())
                .and_then(|member| member.body);
            eprintln!(
                "tok L{} {} paren {} bracket {} top {} body {:?} at {}",
                self.line(index),
                text,
                self.paren,
                self.bracket,
                top,
                body,
                container && self.paren == 0 && self.bracket == 0
            );
        }
        if at_level
            && text == "event"
            && let Some(member) = self
                .levels
                .last_mut()
                .and_then(|level| level.member.as_mut())
        {
            member.saw_event = true;
        }
        if at_level
            && text == "="
            && let Some(member) = self
                .levels
                .last_mut()
                .and_then(|level| level.member.as_mut())
        {
            member.saw_equals = true;
        }
        if at_level
            && text == "=>"
            && let Some(member) = self
                .levels
                .last_mut()
                .and_then(|level| level.member.as_mut())
            && !member.saw_equals
        {
            member.saw_arrow = true;
        }
        if at_level
            && (text == "<" || text == ">")
            && let Some(member) = self
                .levels
                .last_mut()
                .and_then(|level| level.member.as_mut())
            && !member.saw_equals
            && !member.saw_arrow
        {
            if text == "<" {
                member.generic_depth += 1;
            } else if member.generic_depth > 0 {
                member.generic_depth -= 1;
            }
        }
        if at_level
            && text == "("
            && self.opens_parameter_list(index)
            && let Some(member) = self
                .levels
                .last_mut()
                .and_then(|level| level.member.as_mut())
            && !member.saw_equals
            && !member.saw_arrow
            && member.generic_depth == 0
        {
            member.saw_parameter_list = true;
        }
        if at_level
            && text == ";"
            && self
                .levels
                .last()
                .and_then(|level| level.member.as_ref())
                .is_some_and(|member| member.body.is_none())
        {
            let container = self.levels.last().map(|level| level.container).unwrap_or(0);
            let member = self.levels.last_mut().and_then(|level| level.member.take());
            if let Some(member) = member {
                self.finish_member(container, member, index);
            }
        }
        if self
            .levels
            .last()
            .is_some_and(|level| level.kind == Kind::AccessorList)
            && self.paren == 0
            && self.bracket == 0
            && is_accessor_keyword(&text)
        {
            self.levels.last_mut().unwrap().accessors.push(Accessor {
                keyword: index,
                end: None,
                has_body: false,
            });
        }
        if self
            .levels
            .last()
            .is_some_and(|level| level.kind == Kind::AccessorList)
            && text == ";"
            && let Some(accessor) = self
                .levels
                .last_mut()
                .and_then(|level| level.accessors.last_mut())
            && accessor.end.is_none()
        {
            accessor.end = Some(index);
        }
    }

    fn shift_depth(&mut self, index: usize) {
        if !self.counts_as_code(index) {
            return;
        }
        match self.text(index) {
            "(" => self.paren += 1,
            ")" => self.paren -= 1,
            "[" => self.bracket += 1,
            "]" => self.bracket -= 1,
            _ => {}
        }
    }

    /// String text and interpolation holes are not declaration punctuation.
    /// A `)` that is only the tail of `$"...({value})"` must not close a paren.
    fn counts_as_code(&self, index: usize) -> bool {
        !matches!(
            self.tokens[index].kind,
            TokenKind::InterpolatedText | TokenKind::String | TokenKind::Char
        )
    }

    fn opens_parameter_list(&self, paren: usize) -> bool {
        let Some(prev) = self.prev(paren) else {
            return false;
        };
        if !self.counts_as_code(prev) {
            return false;
        }
        let prev_text = self.text(prev);
        if prev_text == ">" {
            return true;
        }
        if self.tokens[prev].kind == TokenKind::Identifier && !is_modifier(prev_text) {
            return true;
        }
        matches!(
            prev_text,
            "+" | "-" | "*" | "/" | "%" | "&" | "|" | "^" | "<<" | ">>" | "!" | "~" | "++" | "--"
        ) && self.saw_operator_keyword(paren)
    }

    fn saw_operator_keyword(&self, paren: usize) -> bool {
        let start = self
            .levels
            .last()
            .and_then(|level| level.member.as_ref())
            .map(|member| member.start)
            .unwrap_or(0);
        (start..paren).any(|index| self.text(index) == "operator")
    }

    fn finish_brace(&mut self, kind: Kind, open: usize, close: usize) {
        let same = self.line(open) == self.line(close);
        if !same && sa1500_kind(kind) {
            if let Some(prev) = self.prev(open)
                && self.line(prev) == self.line(open)
            {
                self.emit(
                    self.tokens[open].start,
                    "SA1500",
                    "Braces for multi-line statements should not share line",
                );
            }
            if let Some(prev) = self.prev(close)
                && self.line(prev) == self.line(close)
            {
                self.emit(
                    self.tokens[close].start,
                    "SA1500",
                    "Braces for multi-line statements should not share line",
                );
            } else if let Some(next) = self.next(close)
                && self.tokens[next].kind != TokenKind::EndOfFile
                && self.line(next) == self.line(close)
                && !sa1500_next_allowed(self.text(next))
            {
                self.emit(
                    self.tokens[close].start,
                    "SA1500",
                    "Braces for multi-line statements should not share line",
                );
            }
        } else if same
            && (kind == Kind::Statement || kind == Kind::Lambda)
            && !self.lambda_header_same_line(open)
        {
            self.emit(
                self.tokens[open].start,
                "SA1501",
                "Statement should not be on a single line",
            );
        } else if same && self.element_on_one_line(kind, open, close) {
            self.emit(
                self.tokens[open].start,
                "SA1502",
                "Element should not be on a single line",
            );
        }
        if self.closing_preceded_by_blank(close) && sa1508_kind(kind) {
            self.emit(
                self.tokens[close].start,
                "SA1508",
                "A closing brace should not be preceded by a blank line",
            );
        }
        if !same && !self.is_do_open(open) {
            self.closing_followed_by_blank(close);
        }
    }

    fn element_on_one_line(&self, kind: Kind, open: usize, close: usize) -> bool {
        match kind {
            Kind::Method | Kind::Type | Kind::Namespace | Kind::Enum => true,
            Kind::AccessorList => !self.is_auto_accessor_list(open, close),
            _ => false,
        }
    }

    fn is_auto_accessor_list(&self, open: usize, close: usize) -> bool {
        (open + 1..close).all(|index| self.text(index) != "{")
    }

    fn lambda_header_same_line(&self, open: usize) -> bool {
        let mut index = open;
        while index > 0 {
            index -= 1;
            if self.line(index) != self.line(open) {
                return false;
            }
            let text = self.text(index);
            if text == "=>" || text == "delegate" {
                return true;
            }
            if text == ";" || text == "{" || text == "}" {
                return false;
            }
        }
        false
    }

    fn is_do_open(&self, open: usize) -> bool {
        self.prev(open).is_some_and(|prev| self.text(prev) == "do")
    }

    fn closing_followed_by_blank(&mut self, close: usize) {
        let Some(next) = self.next(close) else {
            return;
        };
        if self.tokens[next].kind == TokenKind::EndOfFile {
            return;
        }
        if self.leading_blank_or_quad(next) {
            return;
        }
        if self.same_line(close, next) {
            return;
        }
        let next_text = self.text(next);
        if self.only_whitespace_leading(next)
            && (matches!(
                next_text,
                "}" | "else" | "catch" | "finally" | ";" | "," | ")" | "." | ":" | "]"
            ) || is_accessor_keyword(next_text)
                || self.accessor_modifier(next))
        {
            return;
        }
        if self.starts_with_directive(next) {
            return;
        }
        self.emit(
            self.tokens[close].end,
            "SA1513",
            "Closing brace should be followed by blank line",
        );
    }

    fn accessor_modifier(&self, index: usize) -> bool {
        let text = self.text(index);
        if !matches!(text, "private" | "protected" | "internal" | "public") {
            return false;
        }
        self.next(index).is_some_and(|next| {
            is_accessor_keyword(self.text(next))
                || matches!(
                    self.text(next),
                    "private" | "protected" | "internal" | "public"
                )
        })
    }

    fn leading_blank_or_quad(&self, index: usize) -> bool {
        for trivia in &self.tokens[index].leading {
            match trivia.kind {
                TriviaKind::Whitespace => {}
                TriviaKind::EndOfLine => return true,
                TriviaKind::SingleLineComment => {
                    return self
                        .source
                        .slice(trivia.start, trivia.end)
                        .starts_with("////");
                }
                _ => return false,
            }
        }
        false
    }

    fn only_whitespace_leading(&self, index: usize) -> bool {
        self.tokens[index]
            .leading
            .iter()
            .all(|trivia| trivia.kind == TriviaKind::Whitespace)
    }

    fn starts_with_directive(&self, index: usize) -> bool {
        self.tokens[index]
            .leading
            .iter()
            .any(|trivia| match trivia.kind {
                TriviaKind::Whitespace => false,
                TriviaKind::Directive => true,
                _ => false,
            })
            && self.tokens[index]
                .leading
                .iter()
                .find(|trivia| trivia.kind != TriviaKind::Whitespace)
                .is_some_and(|trivia| trivia.kind == TriviaKind::Directive)
    }

    fn finish_member(&mut self, container: usize, member: Partial, end: usize) {
        let start = member.start;
        let is_using = self.is_using_directive(start, member.body);
        let invalid = self.invalid_member(start, end);
        let kind = if invalid {
            MemberKind::Invalid
        } else if is_using {
            MemberKind::Using
        } else if !member.saw_parameter_list
            && member.body.is_none()
            && !member.saw_event
            && !member.saw_arrow
            && !member.continued
        {
            MemberKind::Field
        } else {
            MemberKind::Other
        };
        let has_doc = self.tokens[start]
            .leading
            .iter()
            .any(|trivia| trivia.kind == TriviaKind::DocComment);
        let single = if kind == MemberKind::Field {
            self.field_decl_single(start, end)
        } else {
            self.line(start) == self.line(end)
        };
        self.members.push(Member {
            container,
            start,
            end,
            kind,
            single,
            has_doc,
        });
        if std::env::var_os("SC_DEBUG").is_some() {
            eprintln!(
                "end L{}-L{} kind {:?} field_single {} doc {}",
                self.line(start),
                self.line(end),
                kind,
                single,
                has_doc
            );
        }
    }

    fn field_decl_single(&self, start: usize, end: usize) -> bool {
        let mut index = start;
        while self.text(index) == "[" {
            let Some(close) = self.matching_bracket(index) else {
                break;
            };
            let Some(next) = self.next(close) else {
                break;
            };
            index = next;
        }
        self.line(index) == self.line(end)
    }

    fn is_using_directive(&self, start: usize, body: Option<usize>) -> bool {
        if body.is_some() || self.next_is(start, "(") {
            return false;
        }
        let text = self.text(start);
        text == "using"
            || (text == "global"
                && self
                    .next(start)
                    .is_some_and(|next| self.text(next) == "using"))
    }

    fn invalid_member(&self, start: usize, end: usize) -> bool {
        let count = end.saturating_sub(start) + 1;
        count <= 1
            || (count == 2
                && self.text(end) == ";"
                && self.tokens[start].kind == TokenKind::Identifier)
    }

    fn finish_accessors(&mut self, accessors: &[Accessor]) {
        let finished: Vec<(usize, usize, bool)> = accessors
            .iter()
            .filter_map(|accessor| {
                accessor
                    .end
                    .map(|end| (accessor.keyword, end, accessor.has_body))
            })
            .collect();
        if finished.len() >= 2 && finished.iter().all(|item| item.2) {
            let single = |item: &(usize, usize, bool)| {
                self.line(self.accessor_start(item.0)) == self.line(item.1)
            };
            if finished.iter().any(single) && finished.iter().any(|item| !single(item)) {
                self.emit(
                    self.tokens[finished[0].0].start,
                    "SA1504",
                    "All accessors should be single-line or multi-line",
                );
            }
        }
        if finished.len() == 2 && finished.iter().all(|item| item.2) {
            let first_multi =
                self.line(self.accessor_start(finished[0].0)) != self.line(finished[0].1);
            let second_multi =
                self.line(self.accessor_start(finished[1].0)) != self.line(finished[1].1);
            let second = self.accessor_start(finished[1].0);
            if (first_multi || second_multi) && !self.has_empty_line(finished[0].1, second) {
                self.emit(
                    self.leading_start(second),
                    "SA1516",
                    "Elements should be separated by blank line",
                );
            }
        }
    }

    fn accessor_start(&self, keyword: usize) -> usize {
        let mut start = keyword;
        while let Some(prev) = self.prev(start) {
            if is_modifier(self.text(prev)) {
                start = prev;
            } else {
                break;
            }
        }
        start
    }

    fn separate_members(&mut self) {
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for (index, member) in self.members.iter().enumerate() {
            if groups.len() <= member.container {
                groups.resize(member.container + 1, Vec::new());
            }
            groups[member.container].push(index);
        }
        let mut reports = Vec::new();
        for group in groups {
            for pair in group.windows(2) {
                let prev = &self.members[pair[0]];
                let next = &self.members[pair[1]];
                if prev.kind == MemberKind::Invalid || next.kind == MemberKind::Invalid {
                    continue;
                }
                if prev.kind == MemberKind::Using && next.kind == MemberKind::Using {
                    continue;
                }
                if next.has_doc {
                    continue;
                }
                if prev.kind == MemberKind::Field && next.kind == MemberKind::Field && prev.single {
                    continue;
                }
                if self.has_empty_line(prev.end, next.start) {
                    continue;
                }
                reports.push(self.leading_start(next.start));
            }
        }
        for at in reports {
            self.emit(at, "SA1516", "Elements should be separated by blank line");
        }
    }

    fn statements(&mut self) {
        let mut index = 0;
        while index < self.tokens.len() {
            let text = self.text(index);
            if text == "if"
                && self
                    .prev(index)
                    .is_none_or(|prev| self.text(prev) != "else")
                && self.next_is(index, "(")
            {
                self.if_chain(index);
            } else if (matches!(text, "for" | "foreach" | "while" | "lock" | "fixed")
                && self.next_is(index, "(")
                && !(text == "while"
                    && self.prev(index).is_some_and(|prev| self.text(prev) == "}")))
                || (text == "do" && self.prev(index).is_none_or(|prev| self.text(prev) != "."))
            {
                self.child_statement(self.after_header(index));
            } else if text == "using" && self.next_is(index, "(") {
                let child = self.after_header(index);
                let consecutive = self.text(child) == "using" && self.next_is(child, "(");
                if !consecutive {
                    self.child_statement(child);
                }
            }
            index += 1;
        }
    }

    fn if_chain(&mut self, start: usize) {
        let mut clauses = Vec::new();
        let mut keyword = start;
        loop {
            let body = self.after_header(keyword);
            clauses.push(body);
            let end = self.statement_end(body, 32);
            let Some(next) = self.next(end) else {
                break;
            };
            if self.text(next) != "else" {
                break;
            }
            let Some(after) = self.next(next) else {
                break;
            };
            if self.text(after) == "if" {
                keyword = after;
            } else {
                clauses.push(after);
                break;
            }
        }
        let any_block = clauses.iter().any(|clause| self.text(*clause) == "{");
        for clause in clauses {
            if self.text(clause) == "{" {
                continue;
            }
            let end = self.statement_end(clause, 32);
            let multi = self.line(clause) != self.line(end);
            if multi {
                self.emit(
                    self.tokens[clause].start,
                    "SA1519",
                    "Braces should not be omitted from multi-line child statement",
                );
            } else if any_block {
                self.emit(
                    self.tokens[clause].start,
                    "SA1520",
                    "Use braces consistently",
                );
            } else {
                self.emit(
                    self.tokens[clause].start,
                    "SA1503",
                    "Braces should not be omitted",
                );
            }
        }
    }

    fn child_statement(&mut self, child: usize) {
        if child >= self.tokens.len()
            || self.text(child) == "{"
            || self.tokens[child].kind == TokenKind::EndOfFile
        {
            return;
        }
        let end = self.statement_end(child, 32);
        if self.line(child) != self.line(end) {
            self.emit(
                self.tokens[child].start,
                "SA1519",
                "Braces should not be omitted from multi-line child statement",
            );
        } else {
            self.emit(
                self.tokens[child].start,
                "SA1503",
                "Braces should not be omitted",
            );
        }
    }

    fn after_header(&self, keyword: usize) -> usize {
        if self.next_is(keyword, "(")
            && let Some(close) = self.matching_paren(keyword + 1)
        {
            return self.next(close).unwrap_or(close);
        }
        self.next(keyword).unwrap_or(keyword)
    }

    fn statement_end(&self, start: usize, fuel: i32) -> usize {
        if fuel <= 0 || start >= self.tokens.len() {
            return start.min(self.tokens.len().saturating_sub(1));
        }
        let text = self.text(start);
        if text == "{" {
            return self.matching_brace(start).unwrap_or(start);
        }
        if text == "if" {
            let mut end = self.statement_end(self.after_header(start), fuel - 1);
            while let Some(next) = self.next(end) {
                if self.text(next) != "else" {
                    break;
                }
                let Some(after) = self.next(next) else {
                    break;
                };
                if self.text(after) == "if" {
                    end = self.statement_end(self.after_header(after), fuel - 1);
                } else {
                    end = self.statement_end(after, fuel - 1);
                    break;
                }
            }
            return end;
        }
        if matches!(
            text,
            "for" | "foreach" | "while" | "lock" | "fixed" | "using" | "switch"
        ) {
            return self.statement_end(self.after_header(start), fuel - 1);
        }
        if text == "do" {
            let end_body = self.statement_end(self.after_header(start), fuel - 1);
            if let Some(while_kw) = self.next(end_body)
                && self.text(while_kw) == "while"
                && let Some(open) = self.next(while_kw)
                && self.text(open) == "("
                && let Some(close) = self.matching_paren(open)
            {
                return self
                    .next(close)
                    .filter(|semi| self.text(*semi) == ";")
                    .unwrap_or(close);
            }
            return end_body;
        }
        self.scan_semicolon(start)
    }

    fn scan_semicolon(&self, start: usize) -> usize {
        let mut paren = 0i32;
        let mut brace = 0i32;
        let mut bracket = 0i32;
        for index in start..self.tokens.len() {
            if self.holes.contains(&index) {
                continue;
            }
            match self.text(index) {
                "(" => paren += 1,
                ")" => paren -= 1,
                "[" => bracket += 1,
                "]" => bracket -= 1,
                "{" => brace += 1,
                "}" if brace == 0 => return index.saturating_sub(1),
                "}" => brace -= 1,
                ";" if paren == 0 && brace == 0 && bracket == 0 => return index,
                _ => {}
            }
        }
        self.tokens.len().saturating_sub(1)
    }

    fn chained_blanks(&mut self) {
        for index in 0..self.tokens.len() {
            let text = self.text(index).to_string();
            if matches!(text.as_str(), "else" | "catch" | "finally")
                && self.keyword_preceded_by_blank(index)
            {
                let message = format!("'{text}' statement should not be preceded by a blank line");
                self.emit(self.tokens[index].start, "SA1510", &message);
            }
            if text == "while"
                && self
                    .prev(index)
                    .is_some_and(|prev| self.text(prev) == "}" && self.is_do_close(prev))
                && self.keyword_preceded_by_blank(index)
            {
                self.emit(
                    self.tokens[index].start,
                    "SA1511",
                    "While-do footer should not be preceded by blank line",
                );
            }
        }
    }

    fn is_do_close(&self, close: usize) -> bool {
        self.matching_open_brace(close)
            .is_some_and(|open| self.is_do_open(open))
    }

    fn keyword_preceded_by_blank(&self, index: usize) -> bool {
        let trivia = &self.tokens[index].leading;
        let mut cursor = trivia.len() as isize - 1;
        while cursor >= 0 && trivia[cursor as usize].kind == TriviaKind::Whitespace {
            cursor -= 1;
        }
        if cursor < 0 || trivia[cursor as usize].kind != TriviaKind::EndOfLine {
            return false;
        }
        let mut blank = -1i32;
        while cursor >= 0 {
            match trivia[cursor as usize].kind {
                TriviaKind::Whitespace => {}
                TriviaKind::EndOfLine => blank += 1,
                _ => return blank > 0,
            }
            cursor -= 1;
        }
        true
    }

    fn comments(&mut self) {
        for index in 0..self.tokens.len() {
            if !self.tokens[index]
                .leading
                .iter()
                .any(|trivia| trivia.kind == TriviaKind::SingleLineComment)
            {
                continue;
            }
            let mut merged: Vec<&Trivia> = Vec::new();
            if index > 0 {
                merged.extend(self.tokens[index - 1].trailing.iter());
            }
            merged.extend(self.tokens[index].leading.iter());
            for cursor in 0..merged.len() {
                if merged[cursor].kind != TriviaKind::SingleLineComment {
                    continue;
                }
                let start = merged[cursor].start;
                let in_leading = self.tokens[index]
                    .leading
                    .iter()
                    .any(|trivia| trivia.start == start);
                if !in_leading {
                    continue;
                }
                let text = self.source.slice(start, merged[cursor].end);
                if text.starts_with("////")
                    || !on_own_line(&merged, cursor)
                    || is_file_header(&merged, cursor)
                {
                    continue;
                }
                let at_scope = self.comment_at_scope_start(index, start);
                if !at_scope
                    && !preceded_by_blank_trivia(&merged, cursor)
                    && !preceded_by_comment(&merged, cursor)
                    && !preceded_by_directive(&merged, cursor)
                {
                    self.emit(
                        start,
                        "SA1515",
                        "Single-line comment should be preceded by blank line",
                    );
                }
                let (blanks, next) = trailing_blank_count(&merged, cursor);
                if blanks == 1 && next.map(|item| !is_comment_trivia(item)).unwrap_or(true) {
                    self.emit(
                        start,
                        "SA1512",
                        "Single-line comments should not be followed by blank line",
                    );
                }
            }
        }
    }

    fn comment_at_scope_start(&self, token: usize, trivia_start: u32) -> bool {
        if self.tokens[token]
            .trailing
            .iter()
            .any(|trivia| trivia.start == trivia_start)
            && self.text(token) == "{"
        {
            return true;
        }
        self.prev(token)
            .is_some_and(|prev| self.text(prev) == "{" || self.text(prev) == ":")
    }

    fn documentation(&mut self) {
        for index in 0..self.tokens.len() {
            let Some(doc_at) = self.tokens[index]
                .leading
                .iter()
                .position(|trivia| trivia.kind == TriviaKind::DocComment)
            else {
                continue;
            };
            self.doc_followed_by_blank(index, doc_at);
            self.doc_preceded_by_blank(index, doc_at);
        }
    }

    fn doc_followed_by_blank(&mut self, index: usize, doc_at: usize) {
        let leading = &self.tokens[index].leading;
        let mut eols = 0i32;
        for (cursor, trivia) in leading.iter().enumerate().rev() {
            match trivia.kind {
                TriviaKind::Whitespace => {}
                TriviaKind::EndOfLine => eols += 1,
                TriviaKind::SingleLineComment | TriviaKind::MultiLineComment => eols -= 1,
                TriviaKind::DocComment => {
                    if eols > 0 {
                        let at = leading
                            .get(cursor + 1)
                            .map(|item| item.start)
                            .unwrap_or(trivia.end);
                        self.emit(
                            at,
                            "SA1506",
                            "Element documentation headers should not be followed by blank line",
                        );
                    }
                    return;
                }
                _ => return,
            }
            if cursor == doc_at {
                return;
            }
        }
    }

    fn doc_preceded_by_blank(&mut self, index: usize, doc_at: usize) {
        let mut list: Vec<&Trivia> = Vec::new();
        if index > 0 {
            list.extend(self.tokens[index - 1].trailing.iter());
        }
        list.extend(self.tokens[index].leading.iter().take(doc_at));
        let mut eols = 0i32;
        let mut done = false;
        for trivia in list.iter().rev() {
            if trivia.kind == TriviaKind::Directive {
                let name = self.source.slice(trivia.detail_start, trivia.detail_end);
                if !matches!(name, "endif" | "region" | "endregion") {
                    return;
                }
                eols += 1;
                done = true;
                break;
            }
            match trivia.kind {
                TriviaKind::Whitespace => {}
                TriviaKind::EndOfLine => eols += 1,
                _ => {
                    done = true;
                    break;
                }
            }
        }
        if eols >= 2 || (!done && self.prev(index).is_some_and(|prev| self.text(prev) == "{")) {
            return;
        }
        let doc = &self.tokens[index].leading[doc_at];
        let slice = self.source.slice(doc.start, doc.end);
        let at = slice
            .find("///")
            .map(|offset| doc.start + offset as u32)
            .unwrap_or(doc.start);
        self.emit(
            at,
            "SA1514",
            "Element documentation header should be preceded by blank line",
        );
    }

    fn end_of_file(&mut self, mode: NewlineAtEnd) {
        let bytes = self.source.text.as_bytes();
        if bytes.is_empty() {
            return;
        }
        let mut index = bytes.len();
        while index > 0 && is_trailing_whitespace(bytes[index - 1]) {
            index -= 1;
        }
        let trailing = &self.source.text[index..];
        if !newline_violation(trailing, mode) {
            return;
        }
        let message = match mode {
            NewlineAtEnd::Require => "File is required to end with a single newline character",
            NewlineAtEnd::Omit => "File may not end with a newline character",
            NewlineAtEnd::Allow => "Code should not contain blank lines at the end of the file",
        };
        self.emit(index as u32, "SA1518", message);
    }

    fn classify(&self, index: usize, parent: Kind) -> Kind {
        let Some(prev) = self.prev(index) else {
            return Kind::Other;
        };
        let prev_text = self.text(prev);
        if prev_text == "=>" {
            return Kind::Lambda;
        }
        if matches!(prev_text, "=" | "," | "new") {
            return Kind::Initializer;
        }
        if matches!(
            prev_text,
            "try" | "else" | "do" | "finally" | "checked" | "unchecked"
        ) {
            return Kind::Statement;
        }
        if prev_text == "unsafe"
            && parent != Kind::Type
            && parent != Kind::Namespace
            && parent != Kind::Root
        {
            return Kind::Statement;
        }
        if is_accessor_keyword(prev_text) {
            return Kind::Accessor;
        }
        if prev_text == "]" {
            return if self.bracket_is_indexer(prev) {
                Kind::AccessorList
            } else {
                Kind::Initializer
            };
        }
        if let Some(kind) = self.declaration_kind(index) {
            return kind;
        }
        if prev_text == ")" {
            return self.kind_after_paren(prev);
        }
        if let Some(close) = self.paren_before(index) {
            return self.kind_after_paren(close);
        }
        if parent == Kind::Type && self.tokens[prev].kind == TokenKind::Identifier {
            return Kind::AccessorList;
        }
        Kind::Other
    }

    fn declaration_kind(&self, index: usize) -> Option<Kind> {
        let mut paren = 0i32;
        let mut bracket = 0i32;
        let mut angle = 0i32;
        let mut cursor = index;
        while cursor > 0 {
            cursor -= 1;
            let text = self.text(cursor);
            match text {
                ")" => paren += 1,
                "(" => {
                    if paren == 0 {
                        continue;
                    }
                    paren -= 1;
                }
                "]" => bracket += 1,
                "[" => {
                    if bracket > 0 {
                        bracket -= 1;
                    }
                }
                ">" => angle += 1,
                ">>" => angle += 2,
                "<" => angle = (angle - 1).max(0),
                "{" | "}" | ";" if paren == 0 && bracket == 0 && angle == 0 => return None,
                _ if paren == 0 && bracket == 0 && angle == 0 => {
                    if let Some(kind) = declaration_keyword(text)
                        && !self
                            .prev(cursor)
                            .is_some_and(|before| self.text(before) == ":")
                    {
                        return Some(kind);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn kind_after_paren(&self, close_paren: usize) -> Kind {
        let Some(open) = self.matching_open_paren(close_paren) else {
            return Kind::Other;
        };
        let Some(before) = self.prev(open) else {
            return Kind::Method;
        };
        let text = self.text(before);
        if matches!(
            text,
            "if" | "for" | "foreach" | "while" | "switch" | "lock" | "using" | "fixed" | "catch"
        ) {
            return Kind::Statement;
        }
        if text == "delegate" {
            return Kind::Lambda;
        }
        if self.new_before(open) {
            return Kind::Initializer;
        }
        Kind::Method
    }

    fn paren_before(&self, index: usize) -> Option<usize> {
        let mut angle = 0i32;
        let mut cursor = index;
        while cursor > 0 {
            cursor -= 1;
            match self.text(cursor) {
                ">" => angle += 1,
                ">>" => angle += 2,
                "<" => angle = (angle - 1).max(0),
                ")" if angle == 0 => return Some(cursor),
                "{" | "}" | ";" if angle == 0 => return None,
                _ => {}
            }
        }
        None
    }

    fn new_before(&self, open_paren: usize) -> bool {
        let mut cursor = open_paren;
        while cursor > 0 {
            cursor -= 1;
            match self.text(cursor) {
                "new" => return true,
                ";" | "{" | "}" | "=" => return false,
                _ => {}
            }
        }
        false
    }

    fn bracket_is_indexer(&self, close_bracket: usize) -> bool {
        self.matching_open_bracket(close_bracket)
            .and_then(|open| self.prev(open))
            .is_some_and(|prev| self.text(prev) == "this")
    }

    fn preceded_by_blank_gap(&self, index: usize) -> bool {
        let Some(prev) = self.prev(index) else {
            return false;
        };
        eol_gap(&self.tokens[prev].trailing, &self.tokens[index].leading) >= 2
    }

    fn followed_by_blank_gap(&self, open: usize) -> bool {
        let Some(next) = self.next(open) else {
            return false;
        };
        if self.line(next) < self.line(open).saturating_add(2) {
            return false;
        }
        let mut seen = false;
        let mut extra = 0;
        for trivia in self.tokens[open]
            .trailing
            .iter()
            .chain(self.tokens[next].leading.iter())
        {
            match trivia.kind {
                TriviaKind::Whitespace => {}
                TriviaKind::EndOfLine => {
                    if seen {
                        extra += 1;
                    }
                    seen = true;
                }
                _ => {
                    if seen {
                        return extra > 0;
                    }
                }
            }
        }
        extra > 0
    }

    fn closing_preceded_by_blank(&self, close: usize) -> bool {
        let Some(prev) = self.prev(close) else {
            return false;
        };
        eol_gap(&self.tokens[prev].trailing, &self.tokens[close].leading) > 1
    }

    fn has_empty_line(&self, end: usize, start: usize) -> bool {
        let mut previous_eol = false;
        for trivia in self.tokens[end]
            .trailing
            .iter()
            .chain(self.tokens[start].leading.iter())
        {
            match trivia.kind {
                TriviaKind::Whitespace => {}
                TriviaKind::EndOfLine => {
                    if previous_eol {
                        return true;
                    }
                    previous_eol = true;
                }
                _ => previous_eol = false,
            }
        }
        false
    }

    fn matching_brace(&self, open: usize) -> Option<usize> {
        let mut depth = 0i32;
        for index in open..self.tokens.len() {
            if self.holes.contains(&index) || !self.counts_as_code(index) {
                continue;
            }
            match self.text(index) {
                "{" => depth += 1,
                "}" => {
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

    fn matching_open_brace(&self, close: usize) -> Option<usize> {
        let mut depth = 0i32;
        let mut index = close as isize;
        while index >= 0 {
            let current = index as usize;
            if !self.holes.contains(&current) && self.counts_as_code(current) {
                match self.text(current) {
                    "}" => depth += 1,
                    "{" => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(current);
                        }
                    }
                    _ => {}
                }
            }
            index -= 1;
        }
        None
    }

    fn matching_paren(&self, open: usize) -> Option<usize> {
        let mut depth = 0i32;
        for index in open..self.tokens.len() {
            if !self.counts_as_code(index) {
                continue;
            }
            match self.text(index) {
                "(" => depth += 1,
                ")" => {
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

    fn matching_open_paren(&self, close: usize) -> Option<usize> {
        let mut depth = 0i32;
        let mut index = close as isize;
        while index >= 0 {
            if !self.counts_as_code(index as usize) {
                index -= 1;
                continue;
            }
            match self.text(index as usize) {
                ")" => depth += 1,
                "(" => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(index as usize);
                    }
                }
                _ => {}
            }
            index -= 1;
        }
        None
    }

    fn matching_bracket(&self, open: usize) -> Option<usize> {
        let mut depth = 0i32;
        for index in open..self.tokens.len() {
            if !self.counts_as_code(index) {
                continue;
            }
            match self.text(index) {
                "[" => depth += 1,
                "]" => {
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

    fn matching_open_bracket(&self, close: usize) -> Option<usize> {
        let mut depth = 0i32;
        let mut index = close as isize;
        while index >= 0 {
            if !self.counts_as_code(index as usize) {
                index -= 1;
                continue;
            }
            match self.text(index as usize) {
                "]" => depth += 1,
                "[" => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(index as usize);
                    }
                }
                _ => {}
            }
            index -= 1;
        }
        None
    }

    fn text(&self, index: usize) -> &str {
        let token = &self.tokens[index];
        self.source.slice(token.start, token.end)
    }

    fn line(&self, index: usize) -> u32 {
        self.source.line_column(self.tokens[index].start as usize).0
    }

    fn same_line(&self, left: usize, right: usize) -> bool {
        self.line(left) == self.line(right)
    }

    fn prev(&self, index: usize) -> Option<usize> {
        index.checked_sub(1)
    }

    fn next(&self, index: usize) -> Option<usize> {
        let next = index + 1;
        (next < self.tokens.len()).then_some(next)
    }

    fn next_is(&self, index: usize, text: &str) -> bool {
        self.next(index).is_some_and(|next| self.text(next) == text)
    }

    fn leading_start(&self, index: usize) -> u32 {
        self.tokens[index]
            .leading
            .first()
            .map(|trivia| trivia.start)
            .unwrap_or(self.tokens[index].start)
    }

    fn emit(&mut self, at: u32, code: &'static str, message: &str) {
        report(self.source, self.path, at, code, message, self.diagnostics);
    }
}

fn is_container(kind: Kind) -> bool {
    matches!(kind, Kind::Root | Kind::Namespace | Kind::Type)
}

fn is_member_body(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Method | Kind::AccessorList | Kind::Type | Kind::Namespace | Kind::Enum
    )
}

fn sa1500_kind(kind: Kind) -> bool {
    !matches!(kind, Kind::Other)
}

fn sa1508_kind(kind: Kind) -> bool {
    !matches!(kind, Kind::Other | Kind::Lambda)
}

fn sa1500_next_allowed(text: &str) -> bool {
    matches!(text, ")" | "," | ";" | ".")
}

fn is_accessor_keyword(text: &str) -> bool {
    matches!(text, "get" | "set" | "init" | "add" | "remove")
}

fn is_modifier(text: &str) -> bool {
    matches!(
        text,
        "private"
            | "protected"
            | "internal"
            | "public"
            | "sealed"
            | "virtual"
            | "override"
            | "abstract"
            | "readonly"
            | "static"
            | "unsafe"
            | "extern"
            | "volatile"
            | "new"
            | "required"
            | "file"
    )
}

fn declaration_keyword(text: &str) -> Option<Kind> {
    match text {
        "enum" => Some(Kind::Enum),
        "namespace" => Some(Kind::Namespace),
        "class" | "struct" | "interface" | "record" => Some(Kind::Type),
        _ => None,
    }
}

fn eol_gap(trailing: &[Trivia], leading: &[Trivia]) -> i32 {
    let mut eols = 0i32;
    for trivia in trailing.iter().chain(leading.iter()).rev() {
        match trivia.kind {
            TriviaKind::Whitespace => {}
            TriviaKind::EndOfLine => eols += 1,
            TriviaKind::Directive => {
                eols += 1;
                break;
            }
            _ => break,
        }
    }
    eols
}

fn on_own_line(trivia: &[&Trivia], index: usize) -> bool {
    if trivia[index].start == 0 {
        return true;
    }
    let mut cursor = index;
    while cursor > 0 {
        cursor -= 1;
        match trivia[cursor].kind {
            TriviaKind::Whitespace => {}
            TriviaKind::EndOfLine | TriviaKind::Directive | TriviaKind::DocComment => return true,
            _ => return false,
        }
    }
    false
}

fn is_file_header(trivia: &[&Trivia], index: usize) -> bool {
    if trivia.first().is_none_or(|item| item.start != 0) {
        return false;
    }
    let mut in_comment = false;
    for (cursor, item) in trivia.iter().enumerate() {
        match item.kind {
            TriviaKind::Whitespace => {}
            TriviaKind::EndOfLine => {
                if !in_comment {
                    return index < cursor;
                }
                in_comment = false;
            }
            TriviaKind::SingleLineComment => in_comment = true,
            _ => return index < cursor,
        }
    }
    true
}

fn preceded_by_blank_trivia(trivia: &[&Trivia], index: usize) -> bool {
    let mut eols = 0;
    let mut cursor = index;
    while eols < 2 && cursor > 0 {
        cursor -= 1;
        match trivia[cursor].kind {
            TriviaKind::Whitespace => {}
            TriviaKind::EndOfLine => eols += 1,
            TriviaKind::Directive => return eols > 0,
            _ => return false,
        }
    }
    eols >= 2
}

fn preceded_by_comment(trivia: &[&Trivia], index: usize) -> bool {
    let mut eols = 0;
    let mut cursor = index;
    while eols < 2 && cursor > 0 {
        cursor -= 1;
        match trivia[cursor].kind {
            TriviaKind::Whitespace => {}
            TriviaKind::EndOfLine => eols += 1,
            TriviaKind::SingleLineComment => return on_own_line(trivia, cursor),
            TriviaKind::DocComment => return true,
            _ => return false,
        }
    }
    false
}

fn preceded_by_directive(trivia: &[&Trivia], index: usize) -> bool {
    let mut cursor = index;
    while cursor > 0 {
        cursor -= 1;
        match trivia[cursor].kind {
            TriviaKind::Whitespace => {}
            TriviaKind::Directive => return true,
            _ => return false,
        }
    }
    false
}

fn trailing_blank_count<'a>(trivia: &[&'a Trivia], index: usize) -> (i32, Option<&'a Trivia>) {
    let mut eols = 0;
    let mut cursor = index + 1;
    while cursor < trivia.len() {
        match trivia[cursor].kind {
            TriviaKind::Whitespace => cursor += 1,
            TriviaKind::EndOfLine => {
                eols += 1;
                cursor += 1;
            }
            _ => return ((eols - 1).max(0), Some(trivia[cursor])),
        }
    }
    ((eols - 1).max(0), None)
}

fn is_comment_trivia(trivia: &Trivia) -> bool {
    matches!(
        trivia.kind,
        TriviaKind::SingleLineComment | TriviaKind::MultiLineComment | TriviaKind::DocComment
    )
}

fn is_trailing_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

fn newline_violation(trailing: &str, mode: NewlineAtEnd) -> bool {
    let first = trailing.find('\n');
    let second = first.and_then(|index| {
        trailing[index + 1..]
            .find('\n')
            .map(|next| index + 1 + next)
    });
    match mode {
        NewlineAtEnd::Omit => first.is_some(),
        NewlineAtEnd::Require => !first.is_some_and(|index| index + 1 == trailing.len()),
        NewlineAtEnd::Allow => {
            second.is_some() || first.is_some_and(|index| index + 1 != trailing.len())
        }
    }
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
