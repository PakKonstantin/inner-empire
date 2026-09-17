//! Typed frontmatter values.
//!
//! YAML is untyped enough that `rating: 8` and `rating: "8"` are different
//! things, and the difference matters as soon as a query says `rating > 7`.
//! Values are therefore classified once, at parse time, and stored with their
//! type so queries are ordinary comparisons rather than string guessing.

use std::collections::BTreeMap;

use time::format_description::well_known::Rfc3339;
use time::{Date, OffsetDateTime};

/// The type of a property, as the UI's editor needs to know it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PropertyKind {
    Text,
    Number,
    Checkbox,
    Date,
    DateTime,
    List,
    Object,
    Null,
}

impl PropertyKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PropertyKind::Text => "text",
            PropertyKind::Number => "number",
            PropertyKind::Checkbox => "checkbox",
            PropertyKind::Date => "date",
            PropertyKind::DateTime => "datetime",
            PropertyKind::List => "list",
            PropertyKind::Object => "object",
            PropertyKind::Null => "null",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "text" => PropertyKind::Text,
            "number" => PropertyKind::Number,
            "checkbox" => PropertyKind::Checkbox,
            "date" => PropertyKind::Date,
            "datetime" => PropertyKind::DateTime,
            "list" => PropertyKind::List,
            "object" => PropertyKind::Object,
            "null" => PropertyKind::Null,
            _ => return None,
        })
    }
}

/// A frontmatter value, preserving the distinction YAML makes.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum PropertyValue {
    Text(String),
    Number(f64),
    Checkbox(bool),
    /// Calendar date with no time component, e.g. `2026-09-17`.
    Date(String),
    /// An instant, RFC 3339.
    DateTime(String),
    List(Vec<PropertyValue>),
    Object(BTreeMap<String, PropertyValue>),
    Null,
}

impl PropertyValue {
    pub fn kind(&self) -> PropertyKind {
        match self {
            PropertyValue::Text(_) => PropertyKind::Text,
            PropertyValue::Number(_) => PropertyKind::Number,
            PropertyValue::Checkbox(_) => PropertyKind::Checkbox,
            PropertyValue::Date(_) => PropertyKind::Date,
            PropertyValue::DateTime(_) => PropertyKind::DateTime,
            PropertyValue::List(_) => PropertyKind::List,
            PropertyValue::Object(_) => PropertyKind::Object,
            PropertyValue::Null => PropertyKind::Null,
        }
    }

    /// A text rendering used for display, for the `text_value` index column
    /// and for equality filters like `status:active`.
    pub fn as_text(&self) -> String {
        match self {
            PropertyValue::Text(s) => s.clone(),
            PropertyValue::Number(n) => format_number(*n),
            PropertyValue::Checkbox(b) => b.to_string(),
            PropertyValue::Date(d) => d.clone(),
            PropertyValue::DateTime(d) => d.clone(),
            PropertyValue::List(items) => items
                .iter()
                .map(PropertyValue::as_text)
                .collect::<Vec<_>>()
                .join(", "),
            PropertyValue::Object(_) => String::new(),
            PropertyValue::Null => String::new(),
        }
    }

    /// A numeric rendering for range filters. Dates become epoch seconds so
    /// `due < 2026-01-01` works without a second code path.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            PropertyValue::Number(n) => Some(*n),
            PropertyValue::Checkbox(b) => Some(if *b { 1.0 } else { 0.0 }),
            PropertyValue::Date(d) => parse_date(d).map(|date| {
                date.midnight().assume_utc().unix_timestamp() as f64
            }),
            PropertyValue::DateTime(d) => OffsetDateTime::parse(d, &Rfc3339)
                .ok()
                .map(|t| t.unix_timestamp() as f64),
            _ => None,
        }
    }

    /// Flatten to the scalar rows the index stores. A list becomes one row per
    /// item, which is what makes `tags` and other multi-valued properties
    /// searchable with a plain equality filter.
    pub fn flatten_scalars(&self) -> Vec<(Option<usize>, PropertyValue)> {
        match self {
            PropertyValue::List(items) => items
                .iter()
                .enumerate()
                .flat_map(|(i, item)| match item {
                    PropertyValue::List(_) | PropertyValue::Object(_) => Vec::new(),
                    scalar => vec![(Some(i), scalar.clone())],
                })
                .collect(),
            PropertyValue::Object(_) => Vec::new(),
            scalar => vec![(None, scalar.clone())],
        }
    }

    /// Classify a scalar string the way the app's frontmatter rules do.
    /// Shared by the YAML reader and by the properties editor, so a value the
    /// user types is typed identically to one loaded from disk.
    pub fn infer_from_scalar(raw: &str) -> PropertyValue {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return PropertyValue::Null;
        }
        match trimmed {
            "true" | "True" | "TRUE" | "yes" => return PropertyValue::Checkbox(true),
            "false" | "False" | "FALSE" | "no" => return PropertyValue::Checkbox(false),
            "null" | "~" => return PropertyValue::Null,
            _ => {}
        }
        if parse_date(trimmed).is_some() {
            return PropertyValue::Date(trimmed.to_string());
        }
        if OffsetDateTime::parse(trimmed, &Rfc3339).is_ok() {
            return PropertyValue::DateTime(trimmed.to_string());
        }
        // Guard against YAML's "1_000" and leading-zero forms parsing as numbers
        // in ways the user did not intend; require a plain decimal.
        if looks_like_plain_number(trimmed) {
            if let Ok(n) = trimmed.parse::<f64>() {
                return PropertyValue::Number(n);
            }
        }
        PropertyValue::Text(trimmed.to_string())
    }
}

fn looks_like_plain_number(text: &str) -> bool {
    let body = text.strip_prefix('-').unwrap_or(text);
    !body.is_empty()
        && body.chars().all(|c| c.is_ascii_digit() || c == '.')
        && body.chars().filter(|c| *c == '.').count() <= 1
        && body.chars().next().is_some_and(|c| c.is_ascii_digit())
}

fn parse_date(text: &str) -> Option<Date> {
    let format = time::macros::format_description!("[year]-[month]-[day]");
    Date::parse(text, &format).ok()
}

/// Render a float without a trailing `.0` for whole numbers, so `rating: 8`
/// round-trips as `8` rather than `8.0`.
fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// One frontmatter entry, in the order it appeared in the file.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Property {
    pub key: String,
    pub value: PropertyValue,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalars_are_classified_by_shape() {
        assert_eq!(
            PropertyValue::infer_from_scalar("2026-09-17"),
            PropertyValue::Date("2026-09-17".into())
        );
        assert_eq!(
            PropertyValue::infer_from_scalar("2026-09-17T10:30:00Z"),
            PropertyValue::DateTime("2026-09-17T10:30:00Z".into())
        );
        assert_eq!(PropertyValue::infer_from_scalar("8"), PropertyValue::Number(8.0));
        assert_eq!(PropertyValue::infer_from_scalar("8.5"), PropertyValue::Number(8.5));
        assert_eq!(PropertyValue::infer_from_scalar("true"), PropertyValue::Checkbox(true));
        assert_eq!(
            PropertyValue::infer_from_scalar("active"),
            PropertyValue::Text("active".into())
        );
        assert_eq!(PropertyValue::infer_from_scalar(""), PropertyValue::Null);
    }

    #[test]
    fn version_like_text_is_not_mistaken_for_a_number() {
        assert_eq!(
            PropertyValue::infer_from_scalar("1.2.3"),
            PropertyValue::Text("1.2.3".into())
        );
        assert_eq!(
            PropertyValue::infer_from_scalar("v2"),
            PropertyValue::Text("v2".into())
        );
    }

    #[test]
    fn whole_numbers_render_without_a_decimal_point() {
        assert_eq!(PropertyValue::Number(8.0).as_text(), "8");
        assert_eq!(PropertyValue::Number(8.5).as_text(), "8.5");
    }

    #[test]
    fn dates_compare_numerically_so_range_queries_work() {
        let earlier = PropertyValue::Date("2026-01-01".into()).as_number().unwrap();
        let later = PropertyValue::Date("2026-09-17".into()).as_number().unwrap();
        assert!(earlier < later);
    }

    #[test]
    fn a_list_flattens_to_one_indexed_row_per_item() {
        let list = PropertyValue::List(vec![
            PropertyValue::Text("AI".into()),
            PropertyValue::Text("Research".into()),
        ]);
        let rows = list.flatten_scalars();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], (Some(0), PropertyValue::Text("AI".into())));
        assert_eq!(rows[1], (Some(1), PropertyValue::Text("Research".into())));
    }

    #[test]
    fn a_scalar_flattens_to_a_single_unindexed_row() {
        let rows = PropertyValue::Text("active".into()).flatten_scalars();
        assert_eq!(rows, vec![(None, PropertyValue::Text("active".into()))]);
    }
}
