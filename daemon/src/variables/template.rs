use crate::variables::VariableRef;

enum Segment<'a> {
    Literal(&'a str),
    Reference(VariableRef),
}

/// Splits a template into literal text and variable references.
///
/// `$$` is a literal dollar. Anything else that is not a well-formed `$(instance:name)` is left as
/// written: an unmatched `$(` is far more likely to be text the user meant than a binding worth
/// hiding.
fn segments(template: &str) -> Vec<Segment<'_>> {
    let mut segments = Vec::new();
    let mut rest = template;
    while let Some(dollar) = rest.find('$') {
        let (before, from_dollar) = rest.split_at(dollar);
        if !before.is_empty() {
            segments.push(Segment::Literal(before));
        }
        let after_dollar = &from_dollar[1..];
        if let Some(tail) = after_dollar.strip_prefix('$') {
            segments.push(Segment::Literal("$"));
            rest = tail;
            continue;
        }
        let Some(inside) = after_dollar.strip_prefix('(') else {
            segments.push(Segment::Literal("$"));
            rest = after_dollar;
            continue;
        };
        let Some(close) = inside.find(')') else {
            segments.push(Segment::Literal("$("));
            rest = inside;
            continue;
        };
        let (body, tail) = inside.split_at(close);
        let Some(reference) = parse_reference(body) else {
            // Rescan from inside the opening rather than consuming through the closing paren, so a
            // malformed reference cannot swallow a well-formed one that follows it.
            segments.push(Segment::Literal("$("));
            rest = inside;
            continue;
        };
        segments.push(Segment::Reference(reference));
        rest = &tail[1..];
    }
    if !rest.is_empty() {
        segments.push(Segment::Literal(rest));
    }
    segments
}

fn parse_reference(body: &str) -> Option<VariableRef> {
    let (integration_id, name) = body.split_once(':')?;
    if !is_reference_part(integration_id) || !is_reference_part(name) {
        return None;
    }
    Some(VariableRef::new(integration_id, name))
}

fn is_reference_part(part: &str) -> bool {
    !part.is_empty()
        && part
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
}

pub fn interpolate(template: &str, lookup: impl Fn(&VariableRef) -> Option<String>) -> String {
    let mut rendered = String::with_capacity(template.len());
    for segment in segments(template) {
        match segment {
            Segment::Literal(text) => rendered.push_str(text),
            Segment::Reference(reference) => {
                rendered.push_str(&lookup(&reference).unwrap_or_default())
            }
        }
    }
    rendered
}

pub fn references(template: &str) -> Vec<VariableRef> {
    segments(template)
        .into_iter()
        .filter_map(|segment| match segment {
            Segment::Reference(reference) => Some(reference),
            Segment::Literal(_) => None,
        })
        .collect()
}

pub fn has_reference(template: &str) -> bool {
    segments(template)
        .iter()
        .any(|segment| matches!(segment, Segment::Reference(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolving<'a>(
        pairs: &'a [(&'a str, &'a str)],
    ) -> impl Fn(&VariableRef) -> Option<String> + 'a {
        move |reference| {
            pairs.iter().find_map(|(key, value)| {
                (*key == format!("{}:{}", reference.integration_id.0, reference.name))
                    .then(|| (*value).to_string())
            })
        }
    }

    #[test]
    fn a_reference_is_replaced_by_its_value() {
        let rendered = interpolate(
            "now: $(mpris.default:title)",
            resolving(&[("mpris.default:title", "Blue Monday")]),
        );
        assert_eq!(rendered, "now: Blue Monday");
    }

    #[test]
    fn adjacent_references_both_resolve() {
        let rendered = interpolate(
            "$(a.b:one)$(a.b:two)",
            resolving(&[("a.b:one", "1"), ("a.b:two", "2")]),
        );
        assert_eq!(rendered, "12");
    }

    #[test]
    fn an_unknown_variable_renders_as_nothing() {
        let rendered = interpolate("[$(a.b:missing)]", resolving(&[]));
        assert_eq!(rendered, "[]");
    }

    #[test]
    fn a_doubled_dollar_is_a_literal_dollar() {
        let rendered = interpolate("$$5.00", resolving(&[]));
        assert_eq!(rendered, "$5.00");
    }

    #[test]
    fn malformed_references_are_left_as_written() {
        for template in ["$(unterminated", "$(no-colon)", "$(:empty)", "100$", "$ ("] {
            assert_eq!(interpolate(template, resolving(&[])), template);
        }
    }

    #[test]
    fn a_malformed_reference_does_not_swallow_a_valid_one_after_it() {
        let rendered = interpolate("$(broken $(a.b:one)", resolving(&[("a.b:one", "resolved")]));
        assert_eq!(rendered, "$(broken resolved");
    }

    #[test]
    fn references_lists_only_well_formed_bindings() {
        let found = references("$(a.b:one) $(broken $(c.d:two)");
        assert_eq!(
            found,
            vec![
                VariableRef::new("a.b", "one"),
                VariableRef::new("c.d", "two")
            ]
        );
    }
}
