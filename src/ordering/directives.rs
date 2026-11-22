use crate::lex::{Token, TokenKind, TriviaKind, identifier_escape, is_identifier_format};
use crate::source::Source;

#[derive(Debug)]
pub(super) struct UsingScopes {
    pub scopes: Vec<Vec<Directive>>,
    pub root_requires_namespace: bool,
}

#[derive(Debug)]
pub(super) struct Directive {
    pub start: u32,
    pub name: String,
    pub alias: Option<String>,
    pub is_static: bool,
    pub is_system: bool,
    pub alias_qualified: bool,
    pub preceded_by_directive: bool,
}

pub(super) fn collect(source: &Source, tokens: &[Token]) -> UsingScopes {
    let scanner = Scanner { source, tokens };
    let mut result = UsingScopes {
        scopes: vec![Vec::new()],
        root_requires_namespace: true,
    };
    let mut frames = vec![Frame::new(0, false)];
    let mut index = 0;
    while !scanner.eof(index) {
        let frame = frames.last_mut().unwrap();
        let at_root = frame.scope == 0;
        match scanner.text(index) {
            "}" => {
                // A file-scoped namespace ends at the enclosing body's end.
                while frames.last().is_some_and(|frame| frame.file_scoped) {
                    frames.pop();
                }
                if frames.len() > 1 {
                    frames.pop();
                }
                index = scanner.past_optional_semicolon(index + 1);
                continue;
            }
            "using" | "global" => {
                if let Some((end, directive)) = scanner.using_directive(index, at_root) {
                    if frame.in_header {
                        result.scopes[frame.scope].push(directive);
                    }
                    index = end;
                    continue;
                }
            }
            "extern"
                if scanner.text(index + 1) == "alias"
                    && scanner.is_identifier(index + 2)
                    && scanner.text(index + 3) == ";" =>
            {
                index += 4;
                continue;
            }
            "[" if at_root
                && matches!(scanner.text(index + 1), "assembly" | "module")
                && scanner.text(index + 2) == ":" =>
            {
                result.root_requires_namespace = false;
                frame.in_header = false;
                index = scanner.past_balanced(index, "[", "]");
                continue;
            }
            _ => {}
        }

        frame.in_header = false;
        let keyword = scanner.member_keyword(index);
        if let Some((end, file_scoped)) = scanner.namespace_body(keyword) {
            frames.push(Frame::new(result.scopes.len(), file_scoped));
            result.scopes.push(Vec::new());
            index = end;
        } else {
            if at_root && !scanner.is_record_declaration(keyword) {
                result.root_requires_namespace = false;
            }
            index = scanner.past_member(index);
        }
    }
    result
}

struct Frame {
    scope: usize,
    in_header: bool,
    file_scoped: bool,
}

impl Frame {
    fn new(scope: usize, file_scoped: bool) -> Self {
        Self {
            scope,
            in_header: true,
            file_scoped,
        }
    }
}

struct Scanner<'a> {
    source: &'a Source,
    tokens: &'a [Token],
}

#[derive(Default)]
struct Name {
    text: String,
    // Roslyn's System check skips generic names and the unescaped global qualifier.
    first_identifier_is_system: Option<bool>,
    alias_qualified: bool,
}

impl Scanner<'_> {
    fn text(&self, index: usize) -> &str {
        self.tokens
            .get(index)
            .filter(|token| token.kind != TokenKind::InterpolatedText)
            .map_or("", |token| self.source.slice(token.start, token.end))
    }

    fn eof(&self, index: usize) -> bool {
        self.tokens
            .get(index)
            .is_none_or(|token| token.kind == TokenKind::EndOfFile)
    }

    fn is_identifier(&self, index: usize) -> bool {
        self.tokens
            .get(index)
            .is_some_and(|token| token.kind == TokenKind::Identifier)
            && !reserved_keyword(self.text(index))
    }

    fn using_directive(&self, start: usize, at_root: bool) -> Option<(usize, Directive)> {
        let mut index = start;
        let is_global = self.text(index) == "global";
        index += usize::from(is_global);
        if self.text(index) != "using" || self.text(index + 1) == "(" && !is_global && at_root {
            return None;
        }
        index += 1;

        let mut is_static = self.text(index) == "static";
        index += usize::from(is_static);
        if self.text(index) == "unsafe" {
            index += 1;
            if !is_static && self.text(index) == "static" {
                is_static = true;
                index += 1;
            }
        }
        let alias = if self.is_identifier(index) && self.text(index + 1) == "=" {
            let alias = identifier_value(self.text(index));
            index += 2;
            Some(alias)
        } else {
            None
        };

        let target = index;
        let (name_end, mut name) = self.name(target);
        if alias.is_some() {
            index = self.alias_type_end(target, name_end);
            if index != name_end {
                name.text.clear();
                name.alias_qualified |= (name_end..index).any(|part| self.text(part) == "::");
            }
        } else {
            if at_root && !is_global {
                let type_end = self.alias_type_end(target, name_end);
                if predefined_type(self.text(target))
                    || self.is_identifier(type_end)
                    || self.text(target) == "("
                {
                    return None;
                }
            }
            index = name_end;
        }

        let token = &self.tokens[start];
        let is_system = alias.as_deref().map_or_else(
            || name.first_identifier_is_system.unwrap_or(false),
            |alias| alias == "System",
        );
        Some((
            self.past_optional_semicolon(index),
            Directive {
                start: token.start,
                name: name.text,
                alias,
                is_static,
                is_system,
                alias_qualified: name.alias_qualified,
                preceded_by_directive: token
                    .leading
                    .iter()
                    .any(|trivia| trivia.kind == TriviaKind::Directive),
            },
        ))
    }

    fn name(&self, start: usize) -> (usize, Name) {
        let mut index = start;
        let mut name = Name::default();
        let mut omit_type_arguments = false;
        if self.text(index) == "::" || self.is_identifier(index) && self.text(index + 1) == "::" {
            if self.text(index) == "::" {
                name.first_identifier_is_system = Some(false);
            } else {
                let qualifier = self.text(index);
                let value = identifier_value(qualifier);
                if qualifier != "global" {
                    name.first_identifier_is_system = Some(value == "System");
                }
                name.text.push_str(&value);
                index += 1;
            }
            name.text.push_str("::");
            name.alias_qualified = true;
            omit_type_arguments = true;
            index += 1;
        }

        loop {
            if !self.is_identifier(index) {
                name.first_identifier_is_system.get_or_insert(false);
                break;
            }
            let value = identifier_value(self.text(index));
            name.text.push_str(&value);
            index += 1;
            if let Some(end) = self.type_arguments_end(index) {
                let arguments = &self.tokens[index..end];
                if !omit_type_arguments {
                    // Roslyn preserves trivia and spelling inside the type argument list.
                    name.text.push_str(
                        self.source
                            .slice(arguments[0].start, arguments.last().unwrap().end),
                    );
                }
                if name.first_identifier_is_system.is_none() {
                    name.first_identifier_is_system =
                        self.first_type_identifier(index + 1, end - 1);
                }
                name.alias_qualified |= (index + 1..end - 1).any(|part| self.text(part) == "::");
                index = end;
            } else {
                name.first_identifier_is_system
                    .get_or_insert(value == "System");
            }
            if self.text(index) != "." {
                break;
            }
            name.text.push('.');
            index += 1;
            omit_type_arguments = false;
        }
        (index, name)
    }

    fn type_arguments_end(&self, start: usize) -> Option<usize> {
        if self.text(start) != "<" {
            return None;
        }
        let mut depth = 1;
        let mut index = start + 1;
        while !self.eof(index) {
            match self.text(index) {
                ">" => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(index + 1);
                    }
                }
                "<" => depth += 1,
                ";" | "{" | "}" | "=" | "using" | "namespace" => break,
                _ => {}
            }
            index += 1;
        }
        None
    }

    fn first_type_identifier(&self, start: usize, end: usize) -> Option<bool> {
        let mut index = start;
        while index < end {
            let text = self.text(index);
            let next = self.text(index + 1);
            if matches!(text, "managed" | "unmanaged")
                && index >= 2
                && self.text(index - 1) == "*"
                && self.text(index - 2) == "delegate"
            {
                index += 1;
                if next == "[" {
                    index = self.past_balanced(index, "[", "]");
                }
                continue;
            }
            let previous = self.text(index.saturating_sub(1));
            let is_tuple_label = matches!(next, "," | ")")
                && ((self.is_identifier(index.saturating_sub(1)) || predefined_type(previous))
                    || matches!(previous, ">" | "]" | ")" | "?" | "*"));
            if self.is_identifier(index)
                && next != "<"
                && !(text == "global" && next == "::")
                && !is_tuple_label
            {
                return Some(identifier_value(text) == "System");
            }
            index += 1;
        }
        None
    }

    fn alias_type_end(&self, start: usize, name_end: usize) -> usize {
        let mut index = match self.text(start) {
            "(" => self.past_balanced(start, "(", ")"),
            "ref" => {
                let target = start + 1 + usize::from(self.text(start + 1) == "readonly");
                if predefined_type(self.text(target)) {
                    target + 1
                } else {
                    self.name(target).0
                }
            }
            "delegate" if self.text(start + 1) == "*" => {
                let mut arguments = start + 2;
                if matches!(self.text(arguments), "managed" | "unmanaged") {
                    arguments += 1;
                    if self.text(arguments) == "[" {
                        arguments = self.past_balanced(arguments, "[", "]");
                    }
                }
                self.type_arguments_end(arguments).unwrap_or(arguments)
            }
            text if predefined_type(text) => start + 1,
            _ => name_end,
        };
        loop {
            match self.text(index) {
                "*" | "?" => index += 1,
                "[" => index = self.past_balanced(index, "[", "]"),
                _ => return index,
            }
        }
    }

    fn member_keyword(&self, mut index: usize) -> usize {
        loop {
            match self.text(index) {
                "[" => index = self.past_balanced(index, "[", "]"),
                "abstract" | "file" | "internal" | "new" | "partial" | "private" | "protected"
                | "public" | "readonly" | "ref" | "sealed" | "static" | "unsafe" => index += 1,
                _ => return index,
            }
        }
    }

    fn namespace_body(&self, keyword: usize) -> Option<(usize, bool)> {
        if self.text(keyword) != "namespace" {
            return None;
        }
        let mut index = keyword + 1;
        while self.is_identifier(index) {
            index += 1;
            if self.text(index) != "." {
                break;
            }
            index += 1;
        }
        match self.text(index) {
            "{" => Some((index + 1, false)),
            ";" => Some((index + 1, true)),
            _ => None,
        }
    }

    fn is_record_declaration(&self, keyword: usize) -> bool {
        if self.text(keyword) != "record" {
            return false;
        }
        let mut name = keyword + 1;
        if matches!(self.text(name), "class" | "struct") {
            name += 1;
        }
        self.is_identifier(name)
            && (self.eof(name + 1)
                || matches!(self.text(name + 1), "(" | "<" | ":" | "where" | "{" | ";"))
    }

    fn past_member(&self, start: usize) -> usize {
        let mut index = start;
        while !self.eof(index) {
            match self.text(index) {
                "(" => index = self.past_balanced(index, "(", ")"),
                "[" => index = self.past_balanced(index, "[", "]"),
                "{" => return self.past_optional_semicolon(self.past_balanced(index, "{", "}")),
                ";" => return index + 1,
                "}" => return index,
                _ => index += 1,
            }
        }
        index
    }

    fn past_balanced(&self, start: usize, open: &str, close: &str) -> usize {
        let mut depth = 1;
        let mut index = start + 1;
        while !self.eof(index) {
            let text = self.text(index);
            if text == close {
                depth -= 1;
                if depth == 0 {
                    return index + 1;
                }
            } else if text == open {
                depth += 1;
            }
            index += 1;
        }
        index
    }

    fn past_optional_semicolon(&self, index: usize) -> usize {
        index + usize::from(self.text(index) == ";")
    }
}

fn identifier_value(text: &str) -> String {
    let mut remaining = text.strip_prefix('@').unwrap_or(text);
    if remaining.is_ascii() && !remaining.contains('\\') {
        return remaining.to_owned();
    }
    let mut value = String::with_capacity(remaining.len());
    while let Some(ch) = remaining.chars().next() {
        let (ch, width) = identifier_escape(remaining).unwrap_or((ch, ch.len_utf8()));
        if !is_identifier_format(ch) {
            value.push(ch);
        }
        remaining = &remaining[width..];
    }
    value
}

fn predefined_type(text: &str) -> bool {
    matches!(
        text,
        "bool"
            | "byte"
            | "char"
            | "decimal"
            | "double"
            | "float"
            | "int"
            | "long"
            | "object"
            | "sbyte"
            | "short"
            | "string"
            | "uint"
            | "ulong"
            | "ushort"
            | "void"
    )
}

fn reserved_keyword(text: &str) -> bool {
    predefined_type(text)
        || matches!(
            text,
            "abstract"
                | "as"
                | "base"
                | "break"
                | "case"
                | "catch"
                | "checked"
                | "class"
                | "const"
                | "continue"
                | "default"
                | "delegate"
                | "do"
                | "else"
                | "enum"
                | "event"
                | "explicit"
                | "extern"
                | "false"
                | "finally"
                | "fixed"
                | "for"
                | "foreach"
                | "goto"
                | "if"
                | "implicit"
                | "in"
                | "interface"
                | "internal"
                | "is"
                | "lock"
                | "namespace"
                | "new"
                | "null"
                | "operator"
                | "out"
                | "override"
                | "params"
                | "private"
                | "protected"
                | "public"
                | "readonly"
                | "ref"
                | "return"
                | "sealed"
                | "sizeof"
                | "stackalloc"
                | "static"
                | "struct"
                | "switch"
                | "this"
                | "throw"
                | "true"
                | "try"
                | "typeof"
                | "unchecked"
                | "unsafe"
                | "using"
                | "virtual"
                | "volatile"
                | "while"
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lex::lex;

    fn scan(text: &str) -> UsingScopes {
        collect(&Source::new(text.to_owned()), &lex(text))
    }

    fn names(scopes: &UsingScopes) -> Vec<Vec<&str>> {
        scopes
            .scopes
            .iter()
            .map(|scope| {
                scope
                    .iter()
                    .map(|directive| directive.name.as_str())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn finds_each_namespace_header_and_skips_local_and_late_usings() {
        let scopes = scan(
            "global using Root; namespace N; using Outer; namespace A { using Inner; \
             class C { void M() { using var local = Get(); using (local) {} } } \
             using Late; } namespace B { extern alias E; using Other; }",
        );
        assert_eq!(
            names(&scopes),
            vec![vec!["Root"], vec!["Outer"], vec!["Inner"], vec!["Other"]]
        );
        assert_eq!(scopes.scopes[0][0].start, 0);
        assert!(scopes.root_requires_namespace);
    }

    #[test]
    fn root_placement_matches_compilation_unit_child_kinds() {
        for text in [
            "using A;",
            "global using A; namespace N;",
            "using A; [Obsolete] record struct R;",
            "using A; record R { }; namespace N {}",
            "using A; namespace N { class C {} }",
        ] {
            assert!(scan(text).root_requires_namespace, "{text}");
        }
        for text in [
            "using A; [assembly: A]",
            "using A; [module: A]",
            "using A; public class A {}",
            "using A; interface A {}",
            "using A; enum A {}",
            "using A; struct A {}",
            "using A; delegate void A();",
            "using A; using var x = Get();",
            "using A; await using Resource x = Get();",
            "using A; using (Get()) { }",
            "using A; System.Console.WriteLine($\"{{using Fake;}}\");",
            "using A; record.DoSomething();",
            "using A; record x = null;",
        ] {
            let scopes = scan(text);
            assert!(!scopes.root_requires_namespace, "{text}");
            assert_eq!(names(&scopes), vec![vec!["A"]], "{text}");
        }
    }

    #[test]
    fn uses_roslyn_name_and_identifier_value_text() {
        let scopes = scan(
            r"using \u0053ystem.IO;
              using @S\u0079stem;
              using static global::@System.Math;
              using static Foo<@System.X, @A. B >;
              using static System<@B>;
              using static global::X<System>;
              using \u0067lobal::System;
              using unsafe @\u0053ystem = int*;
              using M = (global::System.Int32 X, long Y);",
        );
        let directives = &scopes.scopes[0];
        assert_eq!(
            names(&scopes),
            vec![vec![
                "System.IO",
                "System",
                "global::System.Math",
                "Foo<@System.X, @A. B >",
                "System<@B>",
                "global::X",
                "global::System",
                "",
                "",
            ]]
        );
        assert_eq!(
            directives
                .iter()
                .map(|directive| directive.is_system)
                .collect::<Vec<_>>(),
            vec![true, true, true, true, false, true, false, true, false]
        );
        assert!(directives[2].is_static);
        assert!(directives[2].alias_qualified);
        assert!(directives[5].alias_qualified);
        assert_eq!(directives[7].alias.as_deref(), Some("System"));
        assert_eq!(directives[8].alias.as_deref(), Some("M"));
        assert!(directives[8].alias_qualified);
    }

    #[test]
    fn only_leading_directive_trivia_marks_a_boundary() {
        let scopes = scan(
            "using C;\n#if false\nusing Inactive;\n#endif\n// comment\nusing B;\n\
             using\n#if true\nA;\n#endif\nusing D;\n",
        );
        assert_eq!(names(&scopes), vec![vec!["C", "B", "A", "D"]]);
        assert_eq!(
            scopes.scopes[0]
                .iter()
                .map(|directive| directive.preceded_by_directive)
                .collect::<Vec<_>>(),
            vec![false, true, false, true]
        );
    }

    #[test]
    fn identifiers_preserve_marks_and_omit_format_characters() {
        let scopes = scan(concat!(
            "using Ze\\u0301ta; using Ze\u{0301}ta; ",
            "using @Sys\u{200d}tem; using Sys\\u200Dtem; ",
            "using A\u{0967}\u{093e}\u{203f} = Z;",
        ));
        assert_eq!(
            names(&scopes),
            vec![vec![
                "Ze\u{0301}ta",
                "Ze\u{0301}ta",
                "System",
                "System",
                "Z"
            ]]
        );
        assert!(scopes.scopes[0][2].is_system);
        assert!(scopes.scopes[0][3].is_system);
        assert_eq!(
            scopes.scopes[0][4].alias.as_deref(),
            Some("A\u{0967}\u{093e}\u{203f}")
        );
    }

    #[test]
    fn raw_interpolations_do_not_hide_or_create_using_scopes() {
        let scopes = scan(
            r#"
            namespace A
            {
                using Real;
                class Container
                {
                    string Value = $$"""
                        " } namespace Fake { using Fake; }
                        {{ """nested""" + $"({1})" }}
                        {{{2}}}
                        {{3:00}}
                        """;
                }
            }
            namespace B { using Next; }
            "#,
        );
        assert_eq!(names(&scopes), vec![vec![], vec!["Real"], vec!["Next"]]);
    }
}
