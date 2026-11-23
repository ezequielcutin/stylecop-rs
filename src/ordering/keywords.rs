//! SA1205, SA1206, SA1207, SA1212, and SA1213.
//!
//! Keyword order reads the modifier tokens on one declaration. Accessor order
//! reads the accessor list. Neither one asks the member-order walk to move.

use crate::declarations::{Kind, Parsed};
use crate::source::Source;
use crate::whitespace::Diagnostic;

pub fn diagnostics(parsed: &Parsed, source: &Source, path: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for list in &parsed.lists {
        for member in &list.members {
            if matches!(
                member.kind,
                Kind::Class | Kind::Struct | Kind::Interface | Kind::Record | Kind::RecordStruct
            ) && member.is_partial
                && member.declared_access == crate::declarations::Access::NotSpecified
            {
                emit(
                    source,
                    path,
                    member.name_at,
                    "SA1205",
                    "Partial elements should declare an access modifier",
                    &mut diagnostics,
                );
            }
            if keyword_order_kind(member.kind) {
                sa1206(member, source, path, &mut diagnostics);
            }
            if protected_order_kind(member.kind) {
                sa1207(member, source, path, &mut diagnostics);
            }
            if matches!(member.kind, Kind::Property | Kind::Indexer) {
                sa1212(member, source, path, &mut diagnostics);
            }
            if member.kind == Kind::Event && !member.event_field {
                sa1213(member, source, path, &mut diagnostics);
            }
        }
    }
    diagnostics
}

fn sa1206(
    member: &crate::declarations::Member,
    source: &Source,
    path: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut previous = ModifierType::None;
    let mut other_earlier = false;
    let mut previous_text = "";
    let mut previous_other = "";
    let mut previous_at = 0u32;
    let mut previous_other_at = 0u32;
    for (offset, text) in member.modifier_text.iter().enumerate() {
        let current = modifier_type(text);
        let at = member.modifier_at[offset];
        let report_previous = compare_modifier(current, previous) < 0;
        let report_other = (current == ModifierType::Access || current == ModifierType::Static)
            && previous != ModifierType::Other
            && other_earlier;
        if report_previous || report_other {
            let (reported, _) =
                if report_previous && (!report_other || previous_at < previous_other_at) {
                    (previous_text, previous_at)
                } else {
                    (previous_other, previous_other_at)
                };
            emit(
                source,
                path,
                at,
                "SA1206",
                &format!("The '{text}' modifier should appear before '{reported}'"),
                diagnostics,
            );
        }
        if !other_earlier && current == ModifierType::Other {
            other_earlier = true;
            previous_other = text;
            previous_other_at = at;
        }
        previous = current;
        previous_text = text;
        previous_at = at;
    }
}

fn sa1207(
    member: &crate::declarations::Member,
    source: &Source,
    path: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut saw_protected = false;
    let mut saw_internal = false;
    for (offset, text) in member.modifier_text.iter().enumerate() {
        let at = member.modifier_at[offset];
        if text == "internal" {
            saw_internal = true;
        } else if text == "protected" {
            if saw_internal {
                emit(
                    source,
                    path,
                    at,
                    "SA1207",
                    "The keyword 'protected' should come before 'internal'",
                    diagnostics,
                );
                break;
            }
            saw_protected = true;
        } else if saw_protected && text == "private" {
            emit(
                source,
                path,
                at,
                "SA1207",
                "The keyword 'private' should come before 'protected'",
                diagnostics,
            );
            break;
        }
    }
}

fn sa1212(
    member: &crate::declarations::Member,
    source: &Source,
    path: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if member.accessors.len() != 2 {
        return;
    }
    let first = &member.accessors[0];
    let second = &member.accessors[1];
    if matches!(first.keyword, "set" | "init") && second.keyword == "get" {
        emit(
            source,
            path,
            first.start_at,
            "SA1212",
            "A get accessor appears after a set accessor within a property or indexer",
            diagnostics,
        );
    }
}

fn sa1213(
    member: &crate::declarations::Member,
    source: &Source,
    path: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if member.accessors.len() != 2 {
        return;
    }
    let first = &member.accessors[0];
    let second = &member.accessors[1];
    if first.keyword == "remove" && second.keyword == "add" {
        emit(
            source,
            path,
            first.keyword_at,
            "SA1213",
            "Event accessors should follow order",
            diagnostics,
        );
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ModifierType {
    None,
    Access,
    Static,
    Other,
}

fn modifier_type(text: &str) -> ModifierType {
    match text {
        "public" | "protected" | "internal" | "private" | "file" => ModifierType::Access,
        "static" => ModifierType::Static,
        "virtual" | "abstract" | "override" | "extern" | "unsafe" | "new" | "sealed"
        | "readonly" | "volatile" | "fixed" | "const" | "async" | "partial" | "ref"
        | "required" => ModifierType::Other,
        _ => ModifierType::None,
    }
}

fn compare_modifier(first: ModifierType, second: ModifierType) -> i32 {
    if first == second {
        0
    } else if first == ModifierType::None {
        -1
    } else if second == ModifierType::None {
        1
    } else if (first == ModifierType::Access
        && (second == ModifierType::Static || second == ModifierType::Other))
        || (first == ModifierType::Static && second == ModifierType::Other)
    {
        -1
    } else if (first == ModifierType::Static && second == ModifierType::Access)
        || (first == ModifierType::Other
            && (second == ModifierType::Static || second == ModifierType::Access))
    {
        1
    } else {
        0
    }
}

fn keyword_order_kind(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Class
            | Kind::Struct
            | Kind::Interface
            | Kind::Enum
            | Kind::Delegate
            | Kind::Field
            | Kind::Method
            | Kind::Property
            | Kind::Event
            | Kind::Indexer
            | Kind::Operator
            | Kind::Conversion
            | Kind::Constructor
    )
}

fn protected_order_kind(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Class
            | Kind::Delegate
            | Kind::Event
            | Kind::Field
            | Kind::Indexer
            | Kind::Interface
            | Kind::Method
            | Kind::Property
            | Kind::Struct
    )
}

fn emit(
    source: &Source,
    path: &str,
    byte: u32,
    code: &'static str,
    message: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let (line, column) = source.line_column(byte as usize);
    diagnostics.push(Diagnostic {
        path: path.to_owned(),
        line,
        column,
        code,
        message: message.to_owned(),
    });
}
