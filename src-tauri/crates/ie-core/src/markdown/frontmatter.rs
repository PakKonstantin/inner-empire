//! YAML frontmatter: locating it, typing it, and writing it back without
//! touching the body.
//!
//! The body is byte-for-byte sacred (invariant I1). Every mutation here works
//! by replacing exactly the frontmatter's byte range, so a note whose
//! properties are edited keeps its original line endings, trailing whitespace
//! and everything else the user typed.

use std::collections::BTreeMap;

use crate::model::{Property, PropertyValue};

/// Where the frontmatter block sits in a source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontmatterSpan {
    /// Byte offset of the opening `---`, always 0 when present.
    pub start: usize,
    /// Byte offset just past the closing delimiter's line break. This is where
    /// the body begins.
    pub end: usize,
    /// Byte range of the YAML itself, between the delimiters.
    pub yaml_start: usize,
    pub yaml_end: usize,
}

/// Find the frontmatter block, if the file opens with one.
///
/// The rules are deliberately strict: the very first bytes must be `---`
/// followed by a line break. A `---` in the middle of a file is a horizontal
/// rule and must stay one.
pub fn locate(source: &str) -> Option<FrontmatterSpan> {
    let after_marker = source.strip_prefix("---")?;
    // The opening marker must be alone on its line.
    let yaml_start = if let Some(rest) = after_marker.strip_prefix("\r\n") {
        source.len() - rest.len()
    } else if let Some(rest) = after_marker.strip_prefix('\n') {
        source.len() - rest.len()
    } else {
        return None;
    };

    let mut cursor = yaml_start;
    while cursor <= source.len() {
        let line_end = source[cursor..]
            .find('\n')
            .map(|i| cursor + i)
            .unwrap_or(source.len());
        let line = source[cursor..line_end].trim_end_matches('\r');

        if line == "---" || line == "..." {
            let end = if line_end < source.len() {
                line_end + 1
            } else {
                source.len()
            };
            return Some(FrontmatterSpan {
                start: 0,
                end,
                yaml_start,
                yaml_end: cursor,
            });
        }

        if line_end >= source.len() {
            break;
        }
        cursor = line_end + 1;
    }
    // An unterminated block is not frontmatter; treat the whole file as body so
    // no content is hidden from the user.
    None
}

/// The byte offset at which the note's prose starts.
pub fn body_offset(source: &str) -> usize {
    locate(source).map(|s| s.end).unwrap_or(0)
}

/// Parse the YAML into ordered, typed properties.
///
/// Returns the properties and, separately, any parse error: a malformed block
/// is reported as a diagnostic and the note is still indexed for its links,
/// tags and full text. Refusing to index a note because one YAML key has a
/// stray colon would be worse than the error it reports.
pub fn parse(source: &str) -> (Vec<Property>, Option<String>) {
    let Some(span) = locate(source) else {
        return (Vec::new(), None);
    };
    let yaml = &source[span.yaml_start..span.yaml_end];
    if yaml.trim().is_empty() {
        return (Vec::new(), None);
    }

    match serde_yaml_ng::from_str::<serde_yaml_ng::Value>(yaml) {
        Ok(serde_yaml_ng::Value::Mapping(map)) => {
            let properties = map
                .into_iter()
                .filter_map(|(key, value)| {
                    let key = match key {
                        serde_yaml_ng::Value::String(s) => s,
                        other => scalar_to_string(&other)?,
                    };
                    Some(Property {
                        key,
                        value: from_yaml(&value),
                    })
                })
                .collect();
            (properties, None)
        }
        Ok(serde_yaml_ng::Value::Null) => (Vec::new(), None),
        Ok(_) => (
            Vec::new(),
            Some("frontmatter must be a mapping of keys to values".to_string()),
        ),
        Err(e) => (Vec::new(), Some(e.to_string())),
    }
}

fn scalar_to_string(value: &serde_yaml_ng::Value) -> Option<String> {
    match value {
        serde_yaml_ng::Value::String(s) => Some(s.clone()),
        serde_yaml_ng::Value::Number(n) => Some(n.to_string()),
        serde_yaml_ng::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Convert a YAML value to a typed property value.
///
/// YAML has already decided what is a number and what is a string, and that
/// decision is respected: `rating: 8` is a number, `rating: "8"` is text. The
/// one classification added on top is date recognition, because the YAML 1.2
/// core schema has no date type but the app needs one.
pub fn from_yaml(value: &serde_yaml_ng::Value) -> PropertyValue {
    match value {
        serde_yaml_ng::Value::Null => PropertyValue::Null,
        serde_yaml_ng::Value::Bool(b) => PropertyValue::Checkbox(*b),
        serde_yaml_ng::Value::Number(n) => n
            .as_f64()
            .map(PropertyValue::Number)
            .unwrap_or(PropertyValue::Null),
        serde_yaml_ng::Value::String(s) => match PropertyValue::infer_from_scalar(s) {
            date @ (PropertyValue::Date(_) | PropertyValue::DateTime(_)) => date,
            _ => PropertyValue::Text(s.clone()),
        },
        serde_yaml_ng::Value::Sequence(items) => {
            PropertyValue::List(items.iter().map(from_yaml).collect())
        }
        serde_yaml_ng::Value::Mapping(map) => {
            let mut out = BTreeMap::new();
            for (k, v) in map {
                if let Some(key) = scalar_to_string(k) {
                    out.insert(key, from_yaml(v));
                }
            }
            PropertyValue::Object(out)
        }
        serde_yaml_ng::Value::Tagged(tagged) => from_yaml(&tagged.value),
    }
}

fn to_yaml(value: &PropertyValue) -> serde_yaml_ng::Value {
    match value {
        PropertyValue::Text(s) => serde_yaml_ng::Value::String(s.clone()),
        PropertyValue::Number(n) => {
            serde_yaml_ng::Value::Number(if n.fract() == 0.0 && n.abs() < 1e15 {
                serde_yaml_ng::Number::from(*n as i64)
            } else {
                serde_yaml_ng::Number::from(*n)
            })
        }
        PropertyValue::Checkbox(b) => serde_yaml_ng::Value::Bool(*b),
        PropertyValue::Date(d) => serde_yaml_ng::Value::String(d.clone()),
        PropertyValue::DateTime(d) => serde_yaml_ng::Value::String(d.clone()),
        PropertyValue::List(items) => {
            serde_yaml_ng::Value::Sequence(items.iter().map(to_yaml).collect())
        }
        PropertyValue::Object(map) => {
            let mut out = serde_yaml_ng::Mapping::new();
            for (k, v) in map {
                out.insert(serde_yaml_ng::Value::String(k.clone()), to_yaml(v));
            }
            serde_yaml_ng::Value::Mapping(out)
        }
        PropertyValue::Null => serde_yaml_ng::Value::Null,
    }
}

/// Serialise properties into a frontmatter block, preserving key order.
pub fn render(properties: &[Property]) -> String {
    if properties.is_empty() {
        return String::new();
    }
    let mut map = serde_yaml_ng::Mapping::new();
    for property in properties {
        map.insert(
            serde_yaml_ng::Value::String(property.key.clone()),
            to_yaml(&property.value),
        );
    }
    let body = serde_yaml_ng::to_string(&serde_yaml_ng::Value::Mapping(map)).unwrap_or_default();
    format!("---\n{}---\n", body)
}

/// Replace a note's frontmatter, leaving every byte of the body untouched.
///
/// Passing an empty property list removes the block entirely.
pub fn replace(source: &str, properties: &[Property]) -> String {
    let rendered = render(properties);
    match locate(source) {
        Some(span) => {
            let body = &source[span.end..];
            if rendered.is_empty() {
                body.to_string()
            } else {
                format!("{rendered}{body}")
            }
        }
        None => {
            if rendered.is_empty() {
                source.to_string()
            } else {
                format!("{rendered}{source}")
            }
        }
    }
}

/// Look up one property by key.
pub fn get<'a>(properties: &'a [Property], key: &str) -> Option<&'a PropertyValue> {
    properties
        .iter()
        .find(|p| p.key.eq_ignore_ascii_case(key))
        .map(|p| &p.value)
}

/// Tags declared in frontmatter under `tags` or `tag`.
///
/// Accepts a list, a single string, or a comma/space-separated string, because
/// all three appear in vaults in the wild. A leading `#` is stripped so
/// `tags: [#AI]` and `tags: [AI]` mean the same thing.
pub fn tags(properties: &[Property]) -> Vec<String> {
    let mut out = Vec::new();
    for key in ["tags", "tag"] {
        let Some(value) = get(properties, key) else {
            continue;
        };
        match value {
            PropertyValue::List(items) => {
                for item in items {
                    push_tag(&mut out, &item.as_text());
                }
            }
            PropertyValue::Text(text) => {
                for part in text.split([',', ' ', ';']) {
                    push_tag(&mut out, part);
                }
            }
            _ => {}
        }
    }
    out
}

fn push_tag(out: &mut Vec<String>, raw: &str) {
    let cleaned = raw.trim().trim_start_matches('#').trim_matches('/');
    if cleaned.is_empty() {
        return;
    }
    let owned = cleaned.to_string();
    if !out.contains(&owned) {
        out.push(owned);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "---\ntitle: Example\ntags:\n  - AI\n  - Research\nstatus: active\nrating: 8\ncreated: 2026-09-17\ndone: false\n---\n# Body\n\nText.\n";

    #[test]
    fn locates_a_block_at_the_start_of_the_file() {
        let span = locate(SAMPLE).unwrap();
        assert_eq!(span.start, 0);
        assert_eq!(&SAMPLE[span.end..], "# Body\n\nText.\n");
        assert!(SAMPLE[span.yaml_start..span.yaml_end].starts_with("title: Example"));
    }

    #[test]
    fn a_horizontal_rule_mid_document_is_not_frontmatter() {
        assert!(locate("# Title\n\n---\n\nmore\n").is_none());
    }

    #[test]
    fn an_unterminated_block_is_treated_as_body() {
        assert!(locate("---\ntitle: oops\n\nnever closed\n").is_none());
    }

    #[test]
    fn handles_windows_line_endings() {
        let source = "---\r\ntitle: Example\r\n---\r\nBody\r\n";
        let span = locate(source).unwrap();
        assert_eq!(&source[span.end..], "Body\r\n");
        let (properties, error) = parse(source);
        assert!(error.is_none());
        assert_eq!(get(&properties, "title").unwrap().as_text(), "Example");
    }

    #[test]
    fn parses_every_supported_value_type() {
        let (properties, error) = parse(SAMPLE);
        assert!(error.is_none(), "{error:?}");
        assert_eq!(
            get(&properties, "title"),
            Some(&PropertyValue::Text("Example".into()))
        );
        assert_eq!(
            get(&properties, "status"),
            Some(&PropertyValue::Text("active".into()))
        );
        assert_eq!(
            get(&properties, "rating"),
            Some(&PropertyValue::Number(8.0))
        );
        assert_eq!(
            get(&properties, "created"),
            Some(&PropertyValue::Date("2026-09-17".into()))
        );
        assert_eq!(
            get(&properties, "done"),
            Some(&PropertyValue::Checkbox(false))
        );
        match get(&properties, "tags") {
            Some(PropertyValue::List(items)) => assert_eq!(items.len(), 2),
            other => panic!("expected a list, got {other:?}"),
        }
    }

    #[test]
    fn key_order_from_the_file_is_preserved() {
        let (properties, _) = parse(SAMPLE);
        let keys: Vec<_> = properties.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(
            keys,
            vec!["title", "tags", "status", "rating", "created", "done"]
        );
    }

    #[test]
    fn yaml_typing_is_respected_so_a_quoted_number_stays_text() {
        let (properties, _) = parse("---\na: 8\nb: \"8\"\n---\n");
        assert_eq!(get(&properties, "a"), Some(&PropertyValue::Number(8.0)));
        assert_eq!(
            get(&properties, "b"),
            Some(&PropertyValue::Text("8".into()))
        );
    }

    #[test]
    fn malformed_yaml_is_reported_but_does_not_lose_the_file() {
        let source = "---\ntitle: [unclosed\n---\nBody\n";
        let (properties, error) = parse(source);
        assert!(properties.is_empty());
        assert!(error.is_some());
        // The body is still addressable, so the note remains indexable.
        assert_eq!(&source[body_offset(source)..], "Body\n");
    }

    #[test]
    fn replacing_properties_leaves_the_body_byte_identical() {
        let body = "# Body\n\nText with  trailing spaces.   \n\n\n";
        let source = format!("---\ntitle: Old\n---\n{body}");
        let updated = replace(
            &source,
            &[Property {
                key: "title".into(),
                value: PropertyValue::Text("New".into()),
            }],
        );
        assert!(updated.ends_with(body), "body changed:\n{updated:?}");
        let (properties, _) = parse(&updated);
        assert_eq!(get(&properties, "title").unwrap().as_text(), "New");
    }

    #[test]
    fn properties_can_be_added_to_a_note_that_had_none() {
        let updated = replace(
            "# Just a body\n",
            &[Property {
                key: "status".into(),
                value: PropertyValue::Text("active".into()),
            }],
        );
        assert!(updated.starts_with("---\n"));
        assert!(updated.ends_with("# Just a body\n"));
    }

    #[test]
    fn clearing_every_property_removes_the_block() {
        let updated = replace("---\ntitle: X\n---\nBody\n", &[]);
        assert_eq!(updated, "Body\n");
    }

    #[test]
    fn round_trip_through_render_and_parse_is_stable() {
        let (original, _) = parse(SAMPLE);
        let rendered = render(&original);
        let (reparsed, error) = parse(&rendered);
        assert!(error.is_none());
        assert_eq!(original, reparsed);
    }

    #[test]
    fn frontmatter_tags_accept_lists_strings_and_hash_prefixes() {
        let (list, _) = parse("---\ntags:\n  - AI\n  - \"#Research\"\n---\n");
        assert_eq!(tags(&list), vec!["AI".to_string(), "Research".to_string()]);

        let (inline, _) = parse("---\ntags: AI, Research\n---\n");
        assert_eq!(
            tags(&inline),
            vec!["AI".to_string(), "Research".to_string()]
        );

        let (singular, _) = parse("---\ntag: Solo\n---\n");
        assert_eq!(tags(&singular), vec!["Solo".to_string()]);
    }

    #[test]
    fn nested_objects_survive_a_round_trip() {
        let source = "---\nmeta:\n  author: Ada\n  version: 2\n---\n";
        let (properties, error) = parse(source);
        assert!(error.is_none());
        match get(&properties, "meta") {
            Some(PropertyValue::Object(map)) => {
                assert_eq!(map.get("author"), Some(&PropertyValue::Text("Ada".into())));
                assert_eq!(map.get("version"), Some(&PropertyValue::Number(2.0)));
            }
            other => panic!("expected an object, got {other:?}"),
        }
        let (reparsed, _) = parse(&render(&properties));
        assert_eq!(properties, reparsed);
    }
}
