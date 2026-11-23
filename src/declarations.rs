//! Declaration walk for the syntax rules that read member lists.
//!
//! The pinned analyzers classify Roslyn members. This walk follows the same
//! boundaries: usings are not members, a file-scoped namespace owns the rest of
//! the file, and a body brace is not an initializer.

use std::collections::HashSet;

use crate::lex::{Token, TokenKind};
use crate::source::Source;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Access {
    #[default]
    NotSpecified = 0,
    Private = 1,
    PrivateProtected = 2,
    Protected = 3,
    ProtectedInternal = 4,
    Internal = 5,
    Public = 6,
}

impl Access {
    pub fn name(self) -> &'static str {
        match self {
            Self::NotSpecified => "unspecified access",
            Self::Public => "public",
            Self::Internal => "internal",
            Self::ProtectedInternal => "protected internal",
            Self::Protected => "protected",
            Self::PrivateProtected => "private protected",
            Self::Private => "private",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Namespace,
    Delegate,
    Enum,
    Interface,
    Struct,
    Class,
    Record,
    RecordStruct,
    Field,
    Constructor,
    Destructor,
    Event,
    Property,
    Indexer,
    Conversion,
    Operator,
    Method,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Namespace => "namespace",
            Self::Delegate => "delegate",
            Self::Enum => "enum",
            Self::Interface => "interface",
            Self::Struct => "struct",
            Self::Class => "class",
            Self::Record => "record",
            Self::RecordStruct => "record struct",
            Self::Field => "field",
            Self::Constructor => "constructor",
            Self::Destructor => "destructor",
            Self::Event => "event",
            Self::Property => "property",
            Self::Indexer => "indexer",
            Self::Conversion => "conversion",
            Self::Operator => "operator",
            Self::Method => "method",
        }
    }

    /// Kind used when SA1201 compares two members.
    pub fn order_kind(self) -> Self {
        match self {
            Self::Record => Self::Class,
            Self::RecordStruct => Self::Struct,
            other => other,
        }
    }

    pub fn outer_index(self) -> Option<usize> {
        Some(match self.order_kind() {
            Self::Namespace => 0,
            Self::Delegate => 1,
            Self::Enum => 2,
            Self::Interface => 3,
            Self::Struct => 4,
            Self::Class => 5,
            _ => return None,
        })
    }

    pub fn member_index(self) -> Option<usize> {
        Some(match self.order_kind() {
            Self::Field => 0,
            Self::Constructor => 1,
            Self::Destructor => 2,
            Self::Delegate => 3,
            Self::Event => 4,
            Self::Enum => 5,
            Self::Interface => 6,
            Self::Property => 7,
            Self::Indexer => 8,
            Self::Conversion => 9,
            Self::Operator => 10,
            Self::Method => 11,
            Self::Struct => 12,
            Self::Class => 13,
            _ => return None,
        })
    }

    pub fn ordered_by_access(self) -> bool {
        !matches!(self, Self::Namespace | Self::Destructor)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parent {
    Outer,
    Interface,
    Type,
}

#[derive(Clone, Debug)]
pub struct Accessor {
    pub keyword: &'static str,
    pub start_at: u32,
    pub keyword_at: u32,
}

#[derive(Clone, Debug)]
pub struct Colon {
    pub at: u32,
    pub first_in_line: bool,
    pub trailing_newline: bool,
}

#[derive(Clone, Debug)]
pub struct Member {
    pub kind: Kind,
    pub name_at: u32,
    pub start_at: u32,
    pub declared_access: Access,
    pub is_static: bool,
    pub is_const: bool,
    pub is_readonly: bool,
    pub is_partial: bool,
    pub explicit_interface: bool,
    pub modifier_at: Vec<u32>,
    pub modifier_text: Vec<String>,
    pub variables: Vec<u32>,
    pub event_field: bool,
    pub accessors: Vec<Accessor>,
    pub colon: Option<Colon>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListPlace {
    /// Compilation unit or namespace. SA1204 compares static here; field rules do not.
    Outer,
    /// Class or struct. Every member-order rule reads this list.
    Type,
    /// Interface, record, or record struct. SA1203, SA1204, and SA1214 do not.
    OpenType,
}

#[derive(Clone, Debug)]
pub struct MemberList {
    pub place: ListPlace,
    pub parent: Parent,
    pub members: Vec<Member>,
}

#[derive(Clone, Debug)]
pub struct EnumMember {
    pub name_at: u32,
    pub line: u32,
}

#[derive(Clone, Debug)]
pub struct EnumBody {
    pub start_at: u32,
    pub close_line: u32,
    pub members: Vec<EnumMember>,
    pub trailing_comma: bool,
}

#[derive(Clone, Debug)]
pub struct Initializer {
    pub open_line: u32,
    pub close_line: u32,
    pub last_at: u32,
    pub trailing_comma: bool,
}

#[derive(Clone, Debug)]
pub struct TopType {
    pub name: String,
    pub name_at: u32,
    pub partial: bool,
    /// Class and record class count toward SA1402 by default. Structs do not.
    pub counts_as_class: bool,
}

#[derive(Clone, Debug)]
pub struct Parsed {
    pub code: Vec<usize>,
    pub lists: Vec<MemberList>,
    /// Interiors of statement blocks, as indexes into `code`.
    pub blocks: Vec<(usize, usize)>,
    /// Inclusive byte span of a statement block, from `{` through `}`.
    pub block_spans: Vec<(u32, u32)>,
    pub wheres: Vec<u32>,
    pub enums: Vec<EnumBody>,
    pub initializers: Vec<Initializer>,
    pub top_types: Vec<TopType>,
    pub namespaces: Vec<u32>,
}

pub fn parse(source: &Source, tokens: &[Token]) -> Parsed {
    let code = code_indexes(source, tokens);
    let mut parsed = Parsed {
        code,
        lists: Vec::new(),
        blocks: Vec::new(),
        block_spans: Vec::new(),
        wheres: Vec::new(),
        enums: Vec::new(),
        initializers: Vec::new(),
        top_types: Vec::new(),
        namespaces: Vec::new(),
    };
    let end = parsed.code.len();
    let mut cursor = Cursor {
        source,
        tokens,
        parsed: &mut parsed,
        i: 0,
        end,
    };
    cursor.parse_list(end, ListPlace::Outer, Parent::Outer, None);
    parsed
}

struct Cursor<'a> {
    source: &'a Source,
    tokens: &'a [Token],
    parsed: &'a mut Parsed,
    i: usize,
    end: usize,
}

impl<'a> Cursor<'a> {
    fn parse_list(
        &mut self,
        end: usize,
        place: ListPlace,
        parent: Parent,
        type_name: Option<&str>,
    ) {
        let saved = self.end;
        self.end = end;
        let mut members = Vec::new();
        while self.i < self.end {
            self.skip_loose_directives();
            if self.i >= self.end || self.at() == Some("}") {
                break;
            }
            let before = self.i;
            if let Some(member) = self.parse_member(parent, type_name) {
                let file_scoped =
                    member.kind == Kind::Namespace && self.saw_semicolon_before_brace(before);
                members.push(member);
                if file_scoped {
                    let child_end = self.end;
                    self.parse_list(child_end, ListPlace::Outer, Parent::Outer, None);
                    break;
                }
            } else if self.i == before {
                self.i += 1;
            }
        }
        self.parsed.lists.push(MemberList {
            place,
            parent,
            members,
        });
        self.end = saved;
    }

    fn saw_semicolon_before_brace(&self, start: usize) -> bool {
        let mut i = start;
        while i < self.i {
            match self.text_at(i) {
                "{" => return false,
                ";" => return true,
                _ => {}
            }
            i += 1;
        }
        false
    }

    fn parse_member(&mut self, parent: Parent, type_name: Option<&str>) -> Option<Member> {
        let start = self.i;
        let start_at = self.byte();
        while self.at() == Some("[") && !self.attribute_target_is_assembly() {
            self.skip_pair("[", "]");
        }
        let mods = self.take_modifiers();
        let kind_token = self.at()?.to_owned();
        let member = if kind_token == "namespace" {
            self.parse_namespace(start_at, &mods)
        } else if kind_token == "class" {
            self.parse_type(Kind::Class, start_at, &mods, parent)
        } else if kind_token == "struct" {
            self.parse_type(Kind::Struct, start_at, &mods, parent)
        } else if kind_token == "interface" {
            self.parse_type(Kind::Interface, start_at, &mods, parent)
        } else if kind_token == "enum" {
            self.parse_enum(start_at, &mods, parent)
        } else if kind_token == "delegate" {
            self.parse_delegate(start_at, &mods, parent)
        } else if kind_token == "record" {
            self.parse_record(start_at, &mods, parent)
        } else if kind_token == "event" {
            self.parse_event(start_at, &mods)
        } else if kind_token == "~" {
            self.parse_destructor(start_at, &mods)
        } else if matches!(kind_token.as_str(), "explicit" | "implicit")
            && self.lookahead(1) == Some("operator")
        {
            self.parse_conversion(start_at, &mods)
        } else if let Some(type_name) = type_name
            && kind_token == type_name
            && self.lookahead(1) == Some("(")
        {
            self.parse_constructor(start_at, &mods, type_name)
        } else {
            self.parse_typed(start_at, &mods, type_name)
        };
        if self.i == start {
            self.i += 1;
        }
        member
    }

    fn parse_namespace(&mut self, start_at: u32, mods: &Mods) -> Option<Member> {
        self.bump();
        let name_at = self.byte();
        self.skip_qualified_name();
        let name = self
            .source
            .slice(name_at, self.token_end_before(self.i))
            .to_owned();
        self.parsed.namespaces.push(name_at);
        if self.at() == Some(";") {
            self.bump();
        } else if self.at() == Some("{") {
            let close = self.brace_end();
            self.bump();
            self.parse_list(close, ListPlace::Outer, Parent::Outer, None);
            self.finish_body(close);
        }
        Some(self.member(Kind::Namespace, name_at, start_at, mods, &name, false))
    }

    fn parse_record(&mut self, start_at: u32, mods: &Mods, parent: Parent) -> Option<Member> {
        self.bump();
        let kind = match self.at() {
            Some("struct") => {
                self.bump();
                Kind::RecordStruct
            }
            Some("class") => {
                self.bump();
                Kind::Record
            }
            _ => Kind::Record,
        };
        self.parse_type_after_keyword(kind, start_at, mods, parent)
    }

    fn parse_type(
        &mut self,
        kind: Kind,
        start_at: u32,
        mods: &Mods,
        parent: Parent,
    ) -> Option<Member> {
        self.bump();
        self.parse_type_after_keyword(kind, start_at, mods, parent)
    }

    fn parse_type_after_keyword(
        &mut self,
        kind: Kind,
        start_at: u32,
        mods: &Mods,
        parent: Parent,
    ) -> Option<Member> {
        let name_at = self.byte();
        let name = self.raw_name().to_owned();
        self.bump();
        if self.at() == Some("<") {
            self.skip_angles();
        }
        if self.at() == Some("(") {
            self.skip_pair("(", ")");
        }
        self.skip_type_tail();
        let child_parent = if kind == Kind::Interface {
            Parent::Interface
        } else {
            Parent::Type
        };
        let place = if matches!(kind, Kind::Class | Kind::Struct) {
            ListPlace::Type
        } else {
            ListPlace::OpenType
        };
        if self.at() == Some("{") {
            let close = self.brace_end();
            self.bump();
            if matches!(
                kind,
                Kind::Interface | Kind::Class | Kind::Struct | Kind::Record | Kind::RecordStruct
            ) {
                self.parse_list(close, place, child_parent, Some(&name));
            }
            self.finish_body(close);
        } else if self.at() == Some(";") {
            self.bump();
        }
        if parent == Parent::Outer {
            self.parsed.top_types.push(TopType {
                name: name.clone(),
                name_at,
                partial: mods.partial,
                counts_as_class: matches!(kind, Kind::Class | Kind::Record),
            });
        }
        Some(self.member(kind, name_at, start_at, mods, &name, false))
    }

    fn parse_enum(&mut self, start_at: u32, mods: &Mods, parent: Parent) -> Option<Member> {
        let keyword_at = self.byte();
        self.bump();
        let name_at = self.byte();
        let name = self.raw_name().to_owned();
        self.bump();
        if self.at() == Some("<") {
            self.skip_angles();
        }
        self.skip_type_tail();
        if self.at() == Some("{") {
            let body = self.scan_enum(keyword_at);
            self.parsed.enums.push(body);
        } else if self.at() == Some(";") {
            self.bump();
        }
        if parent == Parent::Outer {
            self.parsed.top_types.push(TopType {
                name: name.clone(),
                name_at,
                partial: mods.partial,
                counts_as_class: false,
            });
        }
        Some(self.member(Kind::Enum, name_at, start_at, mods, &name, false))
    }

    fn parse_delegate(&mut self, start_at: u32, mods: &Mods, parent: Parent) -> Option<Member> {
        self.bump();
        self.skip_type();
        let name_at = self.byte();
        let name = self.raw_name().to_owned();
        if self.at().is_some_and(is_name) {
            self.bump();
        }
        if self.at() == Some("<") {
            self.skip_angles();
        }
        if self.at() == Some("(") {
            self.skip_pair("(", ")");
        }
        self.skip_type_tail();
        if self.at() == Some(";") {
            self.bump();
        }
        if parent == Parent::Outer {
            self.parsed.top_types.push(TopType {
                name: name.clone(),
                name_at,
                partial: false,
                counts_as_class: false,
            });
        }
        Some(self.member(Kind::Delegate, name_at, start_at, mods, &name, false))
    }

    fn parse_event(&mut self, start_at: u32, mods: &Mods) -> Option<Member> {
        self.bump();
        self.skip_type();
        let (name_at, explicit, variables) = self.parse_event_names();
        let event_field = self.at() != Some("{");
        let mut accessors = Vec::new();
        if self.at() == Some("{") {
            accessors = self.parse_accessors();
        } else {
            self.finish_field_tail();
        }
        let mut member = self.member(Kind::Event, name_at, start_at, mods, "", explicit);
        member.event_field = event_field;
        member.variables = variables;
        member.accessors = accessors;
        if event_field {
            // Event fields have no declaration-name location in the pinned analyzer.
            member.name_at = start_at;
        }
        Some(member)
    }

    fn parse_destructor(&mut self, start_at: u32, mods: &Mods) -> Option<Member> {
        self.bump();
        let name_at = self.byte();
        let name = self.raw_name().to_owned();
        if self.at().is_some_and(is_name) {
            self.bump();
        }
        if self.at() == Some("(") {
            self.skip_pair("(", ")");
        }
        self.finish_method_tail();
        Some(self.member(Kind::Destructor, name_at, start_at, mods, &name, false))
    }

    fn parse_conversion(&mut self, start_at: u32, mods: &Mods) -> Option<Member> {
        self.bump();
        if self.at() == Some("operator") {
            self.bump();
        }
        self.skip_type();
        self.finish_method_tail();
        Some(self.member(Kind::Conversion, start_at, start_at, mods, "", false))
    }

    fn parse_constructor(&mut self, start_at: u32, mods: &Mods, type_name: &str) -> Option<Member> {
        let name_at = self.byte();
        self.bump();
        if self.at() == Some("(") {
            self.skip_pair("(", ")");
        }
        let colon = self.finish_method_tail();
        let mut member = self.member(Kind::Constructor, name_at, start_at, mods, type_name, false);
        member.colon = colon;
        Some(member)
    }

    fn parse_typed(
        &mut self,
        start_at: u32,
        mods: &Mods,
        type_name: Option<&str>,
    ) -> Option<Member> {
        self.skip_type();
        if self.at() == Some("operator") {
            self.bump();
            self.skip_operator_token();
            self.finish_method_tail();
            return Some(self.member(Kind::Operator, start_at, start_at, mods, "", false));
        }
        let explicit = self.consume_explicit_interface();
        if self.at() == Some("this") && self.lookahead(1) == Some("[") {
            let name_at = self.byte();
            self.bump();
            self.skip_pair("[", "]");
            let accessors = if self.at() == Some("{") {
                self.parse_accessors()
            } else {
                self.finish_expression_body();
                Vec::new()
            };
            let mut member = self.member(Kind::Indexer, name_at, start_at, mods, "", explicit);
            member.accessors = accessors;
            return Some(member);
        }
        let name_at = self.byte();
        if self.at().is_some_and(is_name) {
            self.bump();
        }
        if self.at() == Some("<") {
            self.skip_angles();
        }
        if self.at() == Some("(") {
            self.skip_pair("(", ")");
            self.finish_method_tail();
            return Some(self.member(
                Kind::Method,
                name_at,
                start_at,
                mods,
                type_name.unwrap_or(""),
                explicit,
            ));
        }
        if self.at() == Some("{") || self.at() == Some("=>") {
            let accessors = if self.at() == Some("{") {
                self.parse_accessors()
            } else {
                self.finish_expression_body();
                Vec::new()
            };
            let mut member = self.member(Kind::Property, name_at, start_at, mods, "", explicit);
            member.variables = vec![name_at];
            member.accessors = accessors;
            if self.at() == Some("=") {
                self.bump();
                self.skip_expression_semicolon();
            }
            return Some(member);
        }
        let mut variables = vec![name_at];
        self.finish_declarators(&mut variables);
        let mut member = self.member(Kind::Field, name_at, start_at, mods, "", false);
        member.variables = variables;
        Some(member)
    }

    fn parse_event_names(&mut self) -> (u32, bool, Vec<u32>) {
        let explicit = self.consume_explicit_interface();
        let name_at = self.byte();
        let mut variables = Vec::new();
        if self.at().is_some_and(is_name) {
            variables.push(self.byte());
            self.bump();
        }
        while self.at() == Some(",") {
            self.bump();
            if self.at().is_some_and(is_name) {
                variables.push(self.byte());
                self.bump();
            }
        }
        (name_at, explicit, variables)
    }

    fn consume_explicit_interface(&mut self) -> bool {
        let mut explicit = false;
        while self.dot_after_name() {
            explicit = true;
            self.bump();
            if self.at() == Some("<") {
                self.skip_angles();
            }
            if matches!(self.at(), Some("." | "::")) {
                self.bump();
            }
        }
        explicit
    }

    fn finish_method_tail(&mut self) -> Option<Colon> {
        let mut colon = None;
        if self.at() == Some("(") {
            self.skip_pair("(", ")");
        }
        if self.at() == Some(":") {
            colon = Some(self.capture_colon());
            self.bump();
            self.skip_until_member_end();
        }
        self.skip_constraints();
        if self.at() == Some("{") {
            self.consume_statement_block();
        } else if self.at() == Some("=>") {
            self.finish_expression_body();
        } else if self.at() == Some(";") {
            self.bump();
        }
        colon
    }

    fn finish_expression_body(&mut self) {
        if self.at() == Some("=>") {
            self.bump();
            self.skip_expression_semicolon();
        }
    }

    fn finish_field_tail(&mut self) {
        let mut variables = Vec::new();
        self.finish_declarators(&mut variables);
    }

    fn finish_declarators(&mut self, variables: &mut Vec<u32>) {
        loop {
            if self.at() == Some("[") {
                self.skip_pair("[", "]");
            }
            if self.at() == Some("=") {
                self.bump();
                self.skip_expression_value();
            }
            if self.at() == Some(",") {
                self.bump();
                if self.at().is_some_and(is_name) {
                    variables.push(self.byte());
                    self.bump();
                }
                continue;
            }
            if self.at() == Some(";") {
                self.bump();
            }
            break;
        }
    }

    fn parse_accessors(&mut self) -> Vec<Accessor> {
        let close = self.brace_end();
        self.bump();
        let mut accessors = Vec::new();
        while self.i < close {
            if self.at() == Some("}") {
                break;
            }
            let start_at = self.byte();
            self.take_modifiers();
            let keyword = match self.at() {
                Some("get") => "get",
                Some("set") => "set",
                Some("init") => "init",
                Some("add") => "add",
                Some("remove") => "remove",
                _ => {
                    self.i += 1;
                    continue;
                }
            };
            let keyword_at = self.byte();
            self.bump();
            accessors.push(Accessor {
                keyword,
                start_at,
                keyword_at,
            });
            if self.at() == Some("{") {
                self.consume_statement_block();
            } else if self.at() == Some("=>") {
                self.finish_expression_body();
            } else if self.at() == Some(";") {
                self.bump();
            }
        }
        self.finish_body(close);
        accessors
    }

    fn skip_constraints(&mut self) {
        while self.at() == Some("where") {
            self.parsed.wheres.push(self.byte());
            self.bump();
            while self.i < self.end {
                match self.at() {
                    Some("where") | Some("{") | Some("=>") | Some(";") => break,
                    Some("(") => self.skip_pair("(", ")"),
                    Some("[") => self.skip_pair("[", "]"),
                    Some("<") => self.skip_angles(),
                    _ => self.i += 1,
                }
            }
        }
    }

    fn skip_type_tail(&mut self) {
        while self.i < self.end {
            match self.at() {
                Some("{") | Some(";") => break,
                Some("where") => self.skip_constraints(),
                Some("(") => self.skip_pair("(", ")"),
                Some("[") => self.skip_pair("[", "]"),
                Some("<") => self.skip_angles(),
                _ => self.i += 1,
            }
        }
    }

    fn skip_until_member_end(&mut self) {
        let mut paren = 0i32;
        let mut bracket = 0i32;
        let mut angle = 0i32;
        while self.i < self.end {
            match self.at() {
                Some("(") => paren += 1,
                Some(")") => paren -= 1,
                Some("[") => bracket += 1,
                Some("]") => bracket -= 1,
                Some("<") if paren == 0 && bracket == 0 => angle += 1,
                Some(">") if paren == 0 && bracket == 0 && angle > 0 => angle -= 1,
                Some("{" | "=>" | ";" | "where") if paren == 0 && bracket == 0 && angle == 0 => {
                    break;
                }
                _ => {}
            }
            self.i += 1;
        }
    }

    fn skip_expression_semicolon(&mut self) {
        self.skip_expression_value();
        if self.at() == Some(";") {
            self.bump();
        }
    }

    fn skip_expression_value(&mut self) {
        let mut paren = 0i32;
        let mut bracket = 0i32;
        let mut angle = 0i32;
        while self.i < self.end {
            let text = self.at().unwrap_or("");
            if paren == 0 && bracket == 0 && angle == 0 && matches!(text, ";" | "," | "}") {
                break;
            }
            match text {
                "(" => paren += 1,
                ")" => paren -= 1,
                "[" => bracket += 1,
                "]" => bracket -= 1,
                "<" if paren == 0 && bracket == 0 => angle += 1,
                ">" if paren == 0 && bracket == 0 && angle > 0 => angle -= 1,
                "{" if paren == 0 && bracket == 0 && angle == 0 => {
                    if matches!(self.previous_text(), Some("=>" | "delegate")) {
                        self.consume_statement_block();
                    } else {
                        self.consume_initializer();
                    }
                    continue;
                }
                _ => {}
            }
            self.i += 1;
        }
    }

    fn consume_statement_block(&mut self) {
        let close = self.brace_end();
        let open = self.i;
        let open_byte = self.byte();
        let close_byte = if close < self.parsed.code.len() {
            self.tokens[self.parsed.code[close]].end
        } else {
            open_byte
        };
        self.parsed.block_spans.push((open_byte, close_byte));
        self.bump();
        if close >= self.i {
            self.parsed.blocks.push((self.i, close));
        }
        self.i = open;
        self.finish_body(close);
    }

    fn consume_initializer(&mut self) {
        let open_line = self.line();
        let close = self.brace_end();
        let close_line = self.line_of_code(close.saturating_sub(1).max(self.i));
        let mut last_at = None;
        let mut awaiting = true;
        let mut paren = 0i32;
        let mut bracket = 0i32;
        let mut brace = 0i32;
        let mut angle = 0i32;
        self.bump();
        while self.i < close {
            let text = self.at().unwrap_or("");
            match text {
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
        let trailing_comma = awaiting && last_at.is_some();
        if let Some(last_at) = last_at {
            self.parsed.initializers.push(Initializer {
                open_line,
                close_line,
                last_at,
                trailing_comma,
            });
        }
        // Leave the semicolon that ends the surrounding expression for its caller.
        self.i = close;
        if self.at() == Some("}") {
            self.bump();
        }
    }

    fn scan_enum(&mut self, keyword_at: u32) -> EnumBody {
        let close = self.brace_end();
        self.bump();
        let mut members = Vec::new();
        let mut trailing_comma = false;
        while self.i < close {
            if self.at().is_some_and(is_name) && !matches!(self.at(), Some("enum")) {
                members.push(EnumMember {
                    name_at: self.byte(),
                    line: self.line(),
                });
                trailing_comma = false;
                self.bump();
                if self.at() == Some("=") {
                    self.bump();
                    self.skip_expression_value();
                }
                if self.at() == Some(",") {
                    trailing_comma = true;
                    self.bump();
                }
            } else if self.i < close {
                self.i += 1;
            }
        }
        let close_line = self.line_of_code(close.saturating_sub(1).max(self.i));
        self.finish_body(close);
        EnumBody {
            start_at: keyword_at,
            close_line,
            members,
            trailing_comma,
        }
    }

    fn skip_loose_directives(&mut self) {
        while self.i < self.end {
            if self.at() == Some("[") && self.attribute_target_is_assembly() {
                self.skip_pair("[", "]");
                continue;
            }
            if self.at() == Some("global") && self.lookahead(1) == Some("using") {
                self.bump();
            }
            if self.at() == Some("using") {
                self.skip_until_semicolon();
                continue;
            }
            if self.at() == Some("extern") && self.lookahead(1) == Some("alias") {
                self.skip_until_semicolon();
                continue;
            }
            break;
        }
    }

    fn attribute_target_is_assembly(&self) -> bool {
        let mut i = self.i + 1;
        if self.text_at(i) == ":" {
            return false;
        }
        matches!(self.text_at(i), "assembly" | "module") && {
            i += 1;
            self.text_at(i) == ":"
        }
    }

    fn skip_until_semicolon(&mut self) {
        let mut paren = 0i32;
        while self.i < self.end {
            match self.at() {
                Some("(") => paren += 1,
                Some(")") => paren -= 1,
                Some(";") if paren == 0 => {
                    self.bump();
                    return;
                }
                _ => {}
            }
            self.i += 1;
        }
    }

    fn take_modifiers(&mut self) -> Mods {
        let mut mods = Mods::default();
        while let Some(text) = self.at() {
            if !is_modifier(text) {
                break;
            }
            mods.tokens.push(self.byte());
            mods.texts.push(text.to_owned());
            match text {
                "public" => mods.access = Access::Public,
                "private" => {
                    mods.access = if mods.saw_protected {
                        Access::PrivateProtected
                    } else {
                        mods.saw_private = true;
                        Access::NotSpecified
                    };
                }
                "internal" => {
                    mods.access = if mods.saw_protected {
                        Access::ProtectedInternal
                    } else {
                        mods.saw_internal = true;
                        Access::NotSpecified
                    };
                }
                "protected" => {
                    mods.access = if mods.saw_internal {
                        Access::ProtectedInternal
                    } else if mods.saw_private {
                        Access::PrivateProtected
                    } else {
                        mods.saw_protected = true;
                        Access::NotSpecified
                    };
                }
                "static" => mods.is_static = true,
                "const" => mods.is_const = true,
                "readonly" => mods.is_readonly = true,
                "partial" => mods.partial = true,
                _ => {}
            }
            self.bump();
        }
        if mods.access == Access::NotSpecified {
            if mods.saw_private {
                mods.access = Access::Private;
            } else if mods.saw_protected {
                mods.access = Access::Protected;
            } else if mods.saw_internal {
                mods.access = Access::Internal;
            }
        }
        mods
    }

    fn member(
        &self,
        kind: Kind,
        name_at: u32,
        start_at: u32,
        mods: &Mods,
        _type_name: &str,
        explicit_interface: bool,
    ) -> Member {
        Member {
            kind,
            name_at,
            start_at,
            declared_access: mods.access,
            is_static: mods.is_static,
            is_const: mods.is_const,
            is_readonly: mods.is_readonly,
            is_partial: mods.partial,
            explicit_interface,
            modifier_at: mods.tokens.clone(),
            modifier_text: mods.texts.clone(),
            variables: Vec::new(),
            event_field: false,
            accessors: Vec::new(),
            colon: None,
        }
    }

    fn capture_colon(&self) -> Colon {
        let token = &self.tokens[self.token_index()];
        Colon {
            at: token.start,
            first_in_line: first_in_line(self.source, token.start),
            trailing_newline: token
                .trailing
                .iter()
                .any(|trivia| trivia.kind == crate::lex::TriviaKind::EndOfLine),
        }
    }

    fn skip_type(&mut self) {
        if matches!(self.at(), Some("ref" | "scoped" | "out" | "in")) {
            self.bump();
        }
        if self.at() == Some("(") {
            self.skip_pair("(", ")");
        } else if self.at().is_some_and(is_name) {
            self.bump();
        } else {
            return;
        }
        loop {
            match self.at() {
                Some("<") => self.skip_angles(),
                Some("[") => self.skip_pair("[", "]"),
                Some("*") | Some("?") => self.bump(),
                Some(".") | Some("::") => {
                    self.bump();
                    if self.at().is_some_and(is_name) {
                        self.bump();
                    }
                }
                _ => break,
            }
        }
    }

    fn skip_qualified_name(&mut self) {
        if self.at().is_some_and(is_name) {
            self.bump();
        }
        while matches!(self.at(), Some("." | "::")) {
            self.bump();
            if self.at().is_some_and(is_name) {
                self.bump();
            }
        }
    }

    fn skip_operator_token(&mut self) {
        if self.at().is_some() {
            self.bump();
        }
    }

    fn skip_pair(&mut self, open: &str, close: &str) {
        if self.at() != Some(open) {
            return;
        }
        let mut depth = 0i32;
        while self.i < self.end {
            let text = self.at().unwrap_or("");
            if text == open {
                depth += 1;
            } else if text == close {
                depth -= 1;
                self.i += 1;
                if depth == 0 {
                    return;
                }
                continue;
            }
            self.i += 1;
        }
    }

    fn skip_angles(&mut self) {
        if self.at() != Some("<") {
            return;
        }
        let mut angle = 0i32;
        let mut paren = 0i32;
        let mut bracket = 0i32;
        while self.i < self.end {
            match self.at().unwrap_or("") {
                "<" if paren == 0 && bracket == 0 => angle += 1,
                ">" if paren == 0 && bracket == 0 => {
                    angle -= 1;
                    self.i += 1;
                    if angle == 0 {
                        return;
                    }
                    continue;
                }
                "(" => paren += 1,
                ")" => paren -= 1,
                "[" => bracket += 1,
                "]" => bracket -= 1,
                _ => {}
            }
            self.i += 1;
        }
    }

    fn brace_end(&self) -> usize {
        let mut depth = 0i32;
        let mut i = self.i;
        while i < self.end {
            match self.text_at(i) {
                "{" => depth += 1,
                "}" => {
                    depth -= 1;
                    if depth == 0 {
                        return i;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        self.end
    }

    fn finish_body(&mut self, close: usize) {
        self.i = close;
        if self.at() == Some("}") {
            self.bump();
        }
        if self.at() == Some(";") {
            self.bump();
        }
    }

    fn dot_after_name(&self) -> bool {
        if !self.at().is_some_and(is_name) || self.at() == Some("this") {
            return false;
        }
        let mut i = self.i + 1;
        if self.text_at(i) == "<" {
            let mut angle = 1i32;
            i += 1;
            while i < self.end && angle > 0 {
                match self.text_at(i) {
                    "<" => angle += 1,
                    ">" => angle -= 1,
                    _ => {}
                }
                i += 1;
            }
        }
        matches!(self.text_at(i), "." | "::")
    }

    fn previous_text(&self) -> Option<&str> {
        if self.i == 0 {
            return None;
        }
        Some(self.text_at(self.i - 1))
    }

    fn at(&self) -> Option<&str> {
        (self.i < self.end).then(|| self.text_at(self.i))
    }

    fn lookahead(&self, offset: usize) -> Option<&str> {
        let index = self.i + offset;
        (index < self.end).then(|| self.text_at(index))
    }

    fn text_at(&self, code_index: usize) -> &str {
        let token = &self.tokens[self.parsed.code[code_index]];
        self.source.slice(token.start, token.end)
    }

    fn bump(&mut self) {
        if self.i < self.end {
            self.i += 1;
        }
    }

    fn byte(&self) -> u32 {
        if self.i >= self.end {
            return self.source.text.len() as u32;
        }
        self.tokens[self.token_index()].start
    }

    fn token_end_before(&self, code_index: usize) -> u32 {
        if code_index == 0 {
            return 0;
        }
        self.tokens[self.parsed.code[code_index - 1]].end
    }

    fn token_index(&self) -> usize {
        self.parsed.code[self.i.min(self.parsed.code.len().saturating_sub(1))]
    }

    fn line(&self) -> u32 {
        self.source.line_column(self.byte() as usize).0
    }

    fn line_of_code(&self, code_index: usize) -> u32 {
        let token = self.parsed.code.get(code_index).copied().unwrap_or(0);
        let start = self.tokens.get(token).map(|token| token.start).unwrap_or(0);
        self.source.line_column(start as usize).0
    }

    fn raw_name(&self) -> &str {
        self.at().unwrap_or("")
    }
}

#[derive(Clone, Debug, Default)]
struct Mods {
    tokens: Vec<u32>,
    texts: Vec<String>,
    access: Access,
    saw_private: bool,
    saw_protected: bool,
    saw_internal: bool,
    is_static: bool,
    is_const: bool,
    is_readonly: bool,
    partial: bool,
}

fn is_modifier(text: &str) -> bool {
    matches!(
        text,
        "public"
            | "private"
            | "protected"
            | "internal"
            | "static"
            | "const"
            | "readonly"
            | "volatile"
            | "unsafe"
            | "new"
            | "virtual"
            | "override"
            | "abstract"
            | "sealed"
            | "partial"
            | "async"
            | "extern"
            | "required"
            | "file"
            | "ref"
            | "fixed"
    )
}

fn is_name(text: &str) -> bool {
    text.starts_with(|ch: char| ch == '_' || ch == '@' || ch.is_ascii_alphabetic())
}

pub fn first_in_line(source: &Source, byte: u32) -> bool {
    let (line, _) = source.line_column(byte as usize);
    let (start, _) = source.line_content(line as usize - 1);
    source.text[start..byte as usize]
        .chars()
        .all(crate::source::is_cs_whitespace)
}

fn code_indexes(source: &Source, tokens: &[Token]) -> Vec<usize> {
    let holes = holes(source, tokens);
    tokens
        .iter()
        .enumerate()
        .filter(|(index, token)| {
            !matches!(
                token.kind,
                TokenKind::InterpolatedText | TokenKind::EndOfFile
            ) && !holes.contains(index)
        })
        .map(|(index, _)| index)
        .collect()
}

fn holes(source: &Source, tokens: &[Token]) -> HashSet<usize> {
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
