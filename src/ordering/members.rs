//! SA1201, SA1202, SA1203, SA1204, and SA1214.
//!
//! These walk one member list. A trait that appears earlier in
//! `elementOrder` stops the later comparison when the two members differ.

use crate::config::{OrderingSettings, OrderingTrait};
use crate::declarations::{Access, Kind, ListPlace, Member, MemberList, Parent, Parsed};
use crate::source::Source;
use crate::whitespace::Diagnostic;

pub fn diagnostics(
    parsed: &Parsed,
    source: &Source,
    path: &str,
    settings: &OrderingSettings,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for list in &parsed.lists {
        sa1201(list, source, path, settings, &mut diagnostics);
        sa1202(list, source, path, settings, &mut diagnostics);
        if list.place != ListPlace::OpenType {
            sa1204(list, source, path, settings, &mut diagnostics);
        }
        if list.place == ListPlace::Type {
            sa1203(list, source, path, settings, &mut diagnostics);
            sa1214(list, source, path, settings, &mut diagnostics);
        }
    }
    diagnostics
}

fn sa1201(
    list: &MemberList,
    source: &Source,
    path: &str,
    settings: &OrderingSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(kind_index) = trait_index(settings, OrderingTrait::Kind) else {
        return;
    };
    let members = &list.members;
    for pair in members.windows(2) {
        let (current, next) = (&pair[0], &pair[1]);
        if !earlier_match(
            settings,
            kind_index,
            list.parent,
            current,
            next,
            TraitUse::Kind,
        ) {
            continue;
        }
        let outer = list.place == ListPlace::Outer;
        let index = order_index(current.kind, outer);
        let next_index = order_index(next.kind, outer);
        if index > next_index {
            emit(
                source,
                path,
                next.name_at,
                "SA1201",
                format!(
                    "A {} should not follow a {}",
                    next.kind.name(),
                    current.kind.name()
                ),
                diagnostics,
            );
        }
    }
}

fn sa1202(
    list: &MemberList,
    source: &Source,
    path: &str,
    settings: &OrderingSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(access_index) = trait_index(settings, OrderingTrait::Accessibility) else {
        return;
    };
    let mut previous: Option<&Member> = None;
    for member in &list.members {
        if !member.kind.ordered_by_access() {
            continue;
        }
        if let Some(before) = previous
            && earlier_match(
                settings,
                access_index,
                list.parent,
                before,
                member,
                TraitUse::Access,
            )
            && access(member, list.parent) > access(before, list.parent)
        {
            let current = access(member, list.parent);
            let earlier = access(before, list.parent);
            emit(
                source,
                path,
                member.name_at,
                "SA1202",
                format!(
                    "'{}' members should come before '{}' members",
                    current.name(),
                    earlier.name()
                ),
                diagnostics,
            );
        }
        previous = Some(member);
    }
}

fn sa1203(
    list: &MemberList,
    source: &Source,
    path: &str,
    settings: &OrderingSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(constant_index) = trait_index(settings, OrderingTrait::Constant) else {
        return;
    };
    let mut previous_const = true;
    let mut previous_static = false;
    let mut previous_readonly = false;
    let mut previous_access = Access::NotSpecified;
    let mut seen = false;
    for member in &list.members {
        if member.kind != Kind::Field {
            continue;
        }
        let current_access = access(member, list.parent);
        let current_const = member.is_const;
        let current_readonly = member.is_const || member.is_readonly;
        let current_static = member.is_const || member.is_static;
        if seen {
            let compare = (0..constant_index).all(|index| match settings.traits()[index] {
                OrderingTrait::Accessibility => current_access == previous_access,
                OrderingTrait::Readonly => current_readonly == previous_readonly,
                OrderingTrait::Static => current_static == previous_static,
                OrderingTrait::Kind | OrderingTrait::Constant => true,
            });
            if compare && !previous_const && current_const {
                emit(
                    source,
                    path,
                    member.name_at,
                    "SA1203",
                    "Constant fields should appear before non-constant fields".to_owned(),
                    diagnostics,
                );
            }
        }
        previous_const = current_const;
        previous_readonly = current_readonly;
        previous_static = current_static;
        previous_access = current_access;
        seen = true;
    }
}

fn sa1204(
    list: &MemberList,
    source: &Source,
    path: &str,
    settings: &OrderingSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(static_index) = trait_index(settings, OrderingTrait::Static) else {
        return;
    };
    let mut previous_kind = None;
    let mut previous_access = Access::NotSpecified;
    let mut previous_static = true;
    let mut previous_const = false;
    let mut previous_readonly = false;
    for member in &list.members {
        let current_kind = member.kind;
        let current_access = access(member, list.parent);
        let current_const = member.is_const;
        let current_readonly = current_const || member.is_readonly;
        let current_static = current_const || member.is_static;
        let compare = (0..static_index).all(|index| match settings.traits()[index] {
            OrderingTrait::Accessibility => current_access == previous_access,
            OrderingTrait::Readonly => current_readonly == previous_readonly,
            OrderingTrait::Constant => current_const == previous_const,
            OrderingTrait::Kind => previous_kind == Some(current_kind),
            OrderingTrait::Static => true,
        });
        if compare && current_static && !previous_static {
            emit(
                source,
                path,
                member.name_at,
                "SA1204",
                "Static members should appear before non-static members".to_owned(),
                diagnostics,
            );
        }
        previous_kind = Some(current_kind);
        previous_access = current_access;
        previous_static = current_static;
        previous_const = current_const;
        previous_readonly = current_readonly;
    }
}

fn sa1214(
    list: &MemberList,
    source: &Source,
    path: &str,
    settings: &OrderingSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(readonly_index) = trait_index(settings, OrderingTrait::Readonly) else {
        return;
    };
    let mut previous: Option<(Access, bool, bool, bool)> = None;
    for member in &list.members {
        if member.kind != Kind::Field {
            previous = None;
            continue;
        }
        let current_access = access(member, list.parent);
        let current_const = member.is_const;
        let current_static = current_const || member.is_static;
        let current_readonly = current_const || member.is_readonly;
        if let Some((previous_access, previous_const, previous_static, previous_readonly)) =
            previous
        {
            let compare = (0..readonly_index).all(|index| match settings.traits()[index] {
                OrderingTrait::Accessibility => previous_access == current_access,
                OrderingTrait::Constant => previous_const == current_const,
                OrderingTrait::Static => previous_static == current_static,
                OrderingTrait::Kind | OrderingTrait::Readonly => true,
            });
            if compare && current_readonly && !previous_readonly {
                emit(
                    source,
                    path,
                    member.name_at,
                    "SA1214",
                    "Readonly fields should appear before non-readonly fields".to_owned(),
                    diagnostics,
                );
            }
        }
        previous = Some((
            current_access,
            current_const,
            current_static,
            current_readonly,
        ));
    }
}

enum TraitUse {
    Kind,
    Access,
}

fn earlier_match(
    settings: &OrderingSettings,
    limit: usize,
    parent: Parent,
    left: &Member,
    right: &Member,
    use_kind: TraitUse,
) -> bool {
    for trait_name in settings.traits().iter().take(limit) {
        let differs = match trait_name {
            OrderingTrait::Accessibility => access(left, parent) != access(right, parent),
            OrderingTrait::Static => left.is_static != right.is_static,
            OrderingTrait::Kind => matches!(use_kind, TraitUse::Access) && left.kind != right.kind,
            OrderingTrait::Constant => {
                matches!(use_kind, TraitUse::Access) && left.is_const != right.is_const
            }
            OrderingTrait::Readonly => {
                matches!(use_kind, TraitUse::Access)
                    && (left.is_const || left.is_readonly) != (right.is_const || right.is_readonly)
            }
        };
        if differs {
            return false;
        }
    }
    true
}

fn access(member: &Member, parent: Parent) -> Access {
    if member.kind == Kind::Constructor && member.is_static {
        return Access::Public;
    }
    if member.explicit_interface
        && matches!(member.kind, Kind::Method | Kind::Property | Kind::Indexer)
    {
        return Access::Public;
    }
    if member.declared_access != Access::NotSpecified {
        return member.declared_access;
    }
    match parent {
        Parent::Outer => Access::Internal,
        Parent::Interface => Access::Public,
        Parent::Type => Access::Private,
    }
}

fn order_index(kind: Kind, outer: bool) -> i32 {
    let found = if outer {
        kind.outer_index()
    } else {
        kind.member_index()
    };
    found.map(|index| index as i32).unwrap_or(-1)
}

fn trait_index(settings: &OrderingSettings, trait_name: OrderingTrait) -> Option<usize> {
    settings
        .traits()
        .iter()
        .position(|item| *item == trait_name)
}

fn emit(
    source: &Source,
    path: &str,
    byte: u32,
    code: &'static str,
    message: String,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let (line, column) = source.line_column(byte as usize);
    diagnostics.push(Diagnostic {
        path: path.to_owned(),
        line,
        column,
        code,
        message,
    });
}
