mod directives;

use std::sync::LazyLock;

use icu_collator::options::{CaseLevel, CollatorOptions, Strength};
use icu_collator::{Collator, CollatorBorrowed};

use crate::config::{OrderingSettings, UsingPlacement};
use crate::lex::Token;
use crate::source::Source;
use crate::whitespace::Diagnostic;

use directives::Directive;

pub fn ordering_diagnostics(
    source: &Source,
    tokens: &[Token],
    path: &str,
    settings: &OrderingSettings,
) -> Vec<Diagnostic> {
    let parsed = directives::collect(source, tokens);
    let mut diagnostics = Vec::new();
    for (index, scope) in parsed.scopes.iter().enumerate() {
        let mut emit = |directive: &Directive, code, message: String| {
            let offset = directive.start as usize;
            let line = source.line_index(offset);
            let (line_start, _) = source.line_content(line);
            let column = source.text[line_start..offset].encode_utf16().count() as u32 + 1;
            diagnostics.push(Diagnostic {
                path: path.to_owned(),
                line: line as u32 + 1,
                column,
                code,
                message,
            });
        };

        let placement = match settings.placement {
            UsingPlacement::Inside if index == 0 && parsed.root_requires_namespace => {
                Some("Using directive should appear within a namespace declaration")
            }
            UsingPlacement::Outside if index != 0 => {
                Some("Using directive should appear outside a namespace declaration")
            }
            _ => None,
        };
        if let Some(message) = placement {
            for directive in scope {
                emit(directive, "SA1200", message.to_owned());
            }
        }

        regular_order(scope, settings.system_first, &mut emit);
        alias_order(scope, &mut emit);
        static_order(scope, settings.system_first, &mut emit);
    }
    diagnostics
}

type Emit<'a> = dyn FnMut(&Directive, &'static str, String) + 'a;

fn regular_order(scope: &[Directive], system_first: bool, emit: &mut Emit<'_>) {
    let mut preceding_out_of_order = None;
    let mut previous: [Option<&Directive>; 2] = [None; 2];

    for (index, directive) in scope.iter().enumerate() {
        if directive.preceded_by_directive {
            previous = [None; 2];
        }
        if directive.is_static || directive.alias.is_some() {
            continue;
        }

        let group = usize::from(system_first && directive.is_system && !directive.alias_qualified);
        if group == 1 && !directive.preceded_by_directive && index > 0 {
            let before = &scope[index - 1];
            if !before.is_system || before.alias_qualified || before.is_static {
                preceding_out_of_order.get_or_insert(before);
            }
            if let Some(before) = preceding_out_of_order {
                emit(
                    directive,
                    "SA1208",
                    format!(
                        "Using directive for '{}' should appear before directive for '{}'",
                        directive.name, before.name
                    ),
                );
            }
        }

        if let Some(before) = previous[group]
            && names_out_of_order(&before.name, &directive.name)
        {
            emit(
                before,
                "SA1210",
                "Using directives should be ordered alphabetically by the namespaces".to_owned(),
            );
        }
        previous[group] = Some(directive);
    }
}

fn alias_order(scope: &[Directive], emit: &mut Emit<'_>) {
    for (alias, following) in scope.iter().zip(scope.iter().skip(1)) {
        if alias.alias.is_some()
            && following.alias.is_none()
            && !following.is_static
            && !following.preceded_by_directive
        {
            emit(
                alias,
                "SA1209",
                "Using alias directives should be placed after all using namespace directives"
                    .to_owned(),
            );
        }
    }

    let mut seen: Vec<(&str, Vec<u16>)> = Vec::new();
    for directive in scope {
        if directive.preceded_by_directive {
            seen.clear();
        }
        let Some(alias) = directive.alias.as_deref() else {
            continue;
        };
        let key: Vec<_> = invariant_lowercase(alias).encode_utf16().collect();
        if seen.last().is_some_and(|(_, previous)| *previous > key) {
            let (before, _) = seen.iter().find(|(_, earlier)| *earlier > key).unwrap();
            emit(
                directive,
                "SA1211",
                format!(
                    "Using alias directive for '{alias}' should appear before using alias directive for '{before}'"
                ),
            );
            break;
        }
        seen.push((alias, key));
    }
}

fn static_order(scope: &[Directive], system_first: bool, emit: &mut Emit<'_>) {
    let mut previous_static = None;
    let mut saw_alias = false;
    for directive in scope {
        if directive.preceded_by_directive {
            previous_static = None;
            saw_alias = false;
        }
        let misplaced = if directive.is_static {
            previous_static = Some(directive);
            saw_alias.then_some(directive)
        } else if directive.alias.is_some() {
            saw_alias = true;
            None
        } else {
            previous_static
        };
        if let Some(misplaced) = misplaced {
            emit(
                misplaced,
                "SA1216",
                "Using static directives should be placed at the correct location".to_owned(),
            );
            break;
        }
    }

    let mut previous: [Option<&Directive>; 2] = [None; 2];
    let mut first_other = None;
    for directive in scope {
        if directive.preceded_by_directive {
            previous = [None; 2];
            first_other = None;
        }
        if !directive.is_static {
            continue;
        }
        let group = usize::from(system_first && directive.is_system);
        let misplaced = if group == 1 && first_other.is_some() {
            first_other
        } else {
            previous[group].filter(|before| names_out_of_order(&before.name, &directive.name))
        };
        if let Some(before) = misplaced {
            emit(
                before,
                "SA1217",
                format!(
                    "The using static directive for '{}' should appear after the using static directive for '{}'",
                    before.name, directive.name
                ),
            );
            break;
        }
        if group == 0 {
            first_other.get_or_insert(directive);
        }
        previous[group] = Some(directive);
    }
}

fn invariant_lowercase(name: &str) -> String {
    // .NET invariant casing uses simple per-character mappings and preserves dotted capital I.
    name.chars()
        .map(|character| {
            if character == 'İ' {
                character
            } else {
                character.to_lowercase().next().unwrap_or(character)
            }
        })
        .collect()
}

fn names_out_of_order(first: &str, second: &str) -> bool {
    static INVARIANT_NAME_ORDER: LazyLock<CollatorBorrowed<'static>> = LazyLock::new(|| {
        // Ignore accents and width, using case only to break otherwise equal names.
        let mut options = CollatorOptions::default();
        options.strength = Some(Strength::Primary);
        options.case_level = Some(CaseLevel::On);
        Collator::try_new(Default::default(), options).expect("compiled invariant collation data")
    });
    INVARIANT_NAME_ORDER.compare(first, second).is_gt()
}
