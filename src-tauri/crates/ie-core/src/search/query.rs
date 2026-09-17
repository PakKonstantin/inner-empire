//! The search query language.
//!
//! ```text
//! machine learning          all of these words
//! "machine learning"        this exact phrase
//! -draft                    excluding this word
//! tag:AI                    tagged #AI, or anything nested under it
//! path:Projects             inside this folder
//! file:readme               whose filename contains this
//! ext:png                   with this extension
//! status:active             whose `status` property is exactly this
//! rating>7                  numeric comparison on a property
//! section:"Design notes"    containing a heading like this
//! ```
//!
//! Filters combine with AND; bare terms go to the full-text index. A hand
//! written parser rather than a grammar generator, because the language is
//! small and the error messages matter more than the generality.

use crate::error::{CoreError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Term {
    Word(String),
    Phrase(String),
    /// `-word` or `-"a phrase"`: must not appear.
    Not(Box<Term>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    Equal,
    NotEqual,
    Greater,
    GreaterOrEqual,
    Less,
    LessOrEqual,
}

impl Comparison {
    fn sql(self) -> &'static str {
        match self {
            Comparison::Equal => "=",
            Comparison::NotEqual => "<>",
            Comparison::Greater => ">",
            Comparison::GreaterOrEqual => ">=",
            Comparison::Less => "<",
            Comparison::LessOrEqual => "<=",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filter {
    /// `tag:AI` — matches `#AI` and everything nested beneath it.
    Tag(String),
    /// `path:Projects` — the path contains this segment.
    Path(String),
    /// `file:readme` — the filename contains this.
    File(String),
    Extension(String),
    /// `section:Design` — the note has a heading containing this.
    Section(String),
    /// A frontmatter property comparison.
    Property {
        key: String,
        comparison: Comparison,
        value: String,
    },
    /// `is:unresolved`, `is:orphan`, `is:untagged` — structural predicates.
    Is(Structural),
    /// Negation of any of the above.
    Not(Box<Filter>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Structural {
    /// Contains at least one link that points nowhere.
    Unresolved,
    /// Nothing links to it.
    Orphan,
    Untagged,
    /// Has no outgoing internal links.
    DeadEnd,
}

impl Structural {
    fn parse(value: &str) -> Option<Self> {
        Some(match value.to_lowercase().as_str() {
            "unresolved" => Structural::Unresolved,
            "orphan" | "orphaned" => Structural::Orphan,
            "untagged" => Structural::Untagged,
            "deadend" | "dead-end" => Structural::DeadEnd,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    pub terms: Vec<Term>,
    pub filters: Vec<Filter>,
}

impl Query {
    /// True when the query asks for nothing, so callers can show recents
    /// rather than running an empty search.
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty() && self.filters.is_empty()
    }

    /// True when only the full-text side has work: lets the caller skip
    /// building a filter subquery.
    pub fn is_text_only(&self) -> bool {
        self.filters.is_empty() && !self.terms.is_empty()
    }

    /// Render the FTS5 MATCH expression, or `None` when there are no terms.
    ///
    /// Everything is quoted, so a user typing `AND`, `*` or `"` gets a literal
    /// search rather than a syntax error from SQLite.
    pub fn to_fts_expression(&self) -> Option<String> {
        if self.terms.is_empty() {
            return None;
        }
        let mut parts: Vec<String> = Vec::new();
        for term in &self.terms {
            match term {
                Term::Word(word) => parts.push(quote_fts(word)),
                Term::Phrase(phrase) => parts.push(quote_fts(phrase)),
                Term::Not(inner) => {
                    let text = match inner.as_ref() {
                        Term::Word(w) | Term::Phrase(w) => w,
                        Term::Not(_) => continue,
                    };
                    parts.push(format!("NOT {}", quote_fts(text)));
                }
            }
        }
        if parts.is_empty() {
            return None;
        }
        // FTS5 reads `a NOT b` as a binary operator, so a leading negation
        // needs something to subtract from.
        if parts[0].starts_with("NOT ") {
            parts.insert(0, quote_fts_wildcard());
        }
        Some(parts.join(" AND ").replace(" AND NOT ", " NOT "))
    }
}

/// Quote a term for FTS5. Doubling inner quotes is how FTS5 escapes them.
fn quote_fts(text: &str) -> String {
    format!("\"{}\"", text.replace('"', "\"\""))
}

/// Matches every indexed document, used as the left side of a bare negation.
fn quote_fts_wildcard() -> String {
    // A prefix query on the empty string is rejected, so match any document
    // that has a path, which every indexed note does.
    "path : \"md\" OR path : \"canvas\" OR body : \"\"".to_string()
}

/// Parse a query string.
pub fn parse(input: &str) -> Result<Query> {
    let mut query = Query::default();
    for token in tokenize(input)? {
        match classify(&token)? {
            Classified::Term(term) => query.terms.push(term),
            Classified::Filter(filter) => query.filters.push(filter),
        }
    }
    Ok(query)
}

/// A token: the raw text plus whether it was quoted and whether it was negated.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    text: String,
    quoted: bool,
    negated: bool,
}

fn tokenize(input: &str) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut negated = false;
    let mut in_quotes = false;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                if in_quotes {
                    in_quotes = false;
                    quoted = true;
                } else {
                    in_quotes = true;
                    // `key:"value"` keeps the prefix already collected.
                }
            }
            c if c.is_whitespace() && !in_quotes => {
                if !current.is_empty() || quoted {
                    tokens.push(Token {
                        text: std::mem::take(&mut current),
                        quoted,
                        negated,
                    });
                    quoted = false;
                    negated = false;
                }
            }
            '-' if current.is_empty() && !in_quotes && !negated => {
                // A leading dash negates; a dash inside a word is part of it.
                if chars.peek().is_some_and(|c| !c.is_whitespace()) {
                    negated = true;
                } else {
                    current.push('-');
                }
            }
            other => current.push(other),
        }
    }

    if in_quotes {
        return Err(CoreError::InvalidQuery {
            message: "a quoted phrase was never closed".into(),
        });
    }
    if !current.is_empty() || quoted {
        tokens.push(Token {
            text: current,
            quoted,
            negated,
        });
    }
    Ok(tokens)
}

enum Classified {
    Term(Term),
    Filter(Filter),
}

fn classify(token: &Token) -> Result<Classified> {
    // A quoted token is always a literal phrase unless a prefix preceded the
    // quote, e.g. `section:"Design notes"`.
    if let Some(filter) = parse_filter(token)? {
        return Ok(Classified::Filter(if token.negated {
            Filter::Not(Box::new(filter))
        } else {
            filter
        }));
    }

    let term = if token.quoted || token.text.contains(' ') {
        Term::Phrase(token.text.clone())
    } else {
        Term::Word(token.text.clone())
    };
    Ok(Classified::Term(if token.negated {
        Term::Not(Box::new(term))
    } else {
        term
    }))
}

fn parse_filter(token: &Token) -> Result<Option<Filter>> {
    // Comparison operators bind tighter than `:` so `rating>7` is a comparison
    // and `status:active` is an equality.
    for (symbol, comparison) in [
        (">=", Comparison::GreaterOrEqual),
        ("<=", Comparison::LessOrEqual),
        ("!=", Comparison::NotEqual),
        (">", Comparison::Greater),
        ("<", Comparison::Less),
    ] {
        if let Some(index) = token.text.find(symbol) {
            let key = token.text[..index].trim();
            let value = token.text[index + symbol.len()..].trim();
            if !key.is_empty() && !value.is_empty() && !key.contains(':') {
                return Ok(Some(Filter::Property {
                    key: key.to_string(),
                    comparison,
                    value: value.to_string(),
                }));
            }
        }
    }

    let Some(index) = token.text.find(':') else {
        return Ok(None);
    };
    let key = token.text[..index].trim();
    let value = token.text[index + 1..].trim();
    if key.is_empty() {
        return Ok(None);
    }
    if value.is_empty() {
        return Err(CoreError::InvalidQuery {
            message: format!("{key}: needs a value"),
        });
    }

    Ok(Some(match key.to_lowercase().as_str() {
        "tag" => Filter::Tag(value.trim_start_matches('#').to_string()),
        "path" | "folder" => Filter::Path(value.to_string()),
        "file" | "name" => Filter::File(value.to_string()),
        "ext" | "extension" => Filter::Extension(value.trim_start_matches('.').to_string()),
        "section" | "heading" => Filter::Section(value.to_string()),
        "is" => match Structural::parse(value) {
            Some(structural) => Filter::Is(structural),
            None => {
                return Err(CoreError::InvalidQuery {
                    message: format!(
                        "is:{value} is not recognised — try unresolved, orphan, untagged or dead-end"
                    ),
                })
            }
        },
        // Anything else is a property name, which is what makes
        // `status:active` work without the user declaring anything.
        _ => Filter::Property {
            key: key.to_string(),
            comparison: Comparison::Equal,
            value: value.to_string(),
        },
    }))
}

/// SQL for one filter: a condition over `files f` plus its bound parameters.
pub(crate) fn filter_sql(filter: &Filter) -> (String, Vec<rusqlite::types::Value>) {
    use rusqlite::types::Value;

    match filter {
        Filter::Tag(tag) => {
            let fold = tag.to_lowercase();
            (
                "EXISTS (SELECT 1 FROM tags t WHERE t.file_id = f.id
                  AND (t.tag_fold = ? OR t.tag_fold LIKE ? ESCAPE '\\'))"
                    .into(),
                vec![
                    Value::Text(fold.clone()),
                    Value::Text(format!("{}/%", escape_like(&fold))),
                ],
            )
        }
        Filter::Path(path) => (
            "f.path_fold LIKE ? ESCAPE '\\'".into(),
            vec![Value::Text(format!("%{}%", escape_like(&path.to_lowercase())))],
        ),
        Filter::File(name) => (
            "f.name_fold LIKE ? ESCAPE '\\'".into(),
            vec![Value::Text(format!("%{}%", escape_like(&name.to_lowercase())))],
        ),
        Filter::Extension(ext) => (
            "f.ext = ?".into(),
            vec![Value::Text(ext.to_lowercase())],
        ),
        Filter::Section(section) => (
            "EXISTS (SELECT 1 FROM headings h WHERE h.file_id = f.id
              AND lower(h.text) LIKE ? ESCAPE '\\')"
                .into(),
            vec![Value::Text(format!(
                "%{}%",
                escape_like(&section.to_lowercase())
            ))],
        ),
        Filter::Property {
            key,
            comparison,
            value,
        } => {
            let key_fold = key.to_lowercase();
            // A numeric-looking value compares numerically, which is what makes
            // `rating>7` behave like arithmetic rather than like string order
            // where "10" sorts before "9".
            match value.parse::<f64>() {
                Ok(number) => (
                    format!(
                        "EXISTS (SELECT 1 FROM properties p WHERE p.file_id = f.id
                          AND p.key_fold = ? AND p.num_value IS NOT NULL AND p.num_value {} ?)",
                        comparison.sql()
                    ),
                    vec![Value::Text(key_fold), Value::Real(number)],
                ),
                Err(_) => (
                    format!(
                        "EXISTS (SELECT 1 FROM properties p WHERE p.file_id = f.id
                          AND p.key_fold = ? AND p.text_fold {} ?)",
                        comparison.sql()
                    ),
                    vec![Value::Text(key_fold), Value::Text(value.to_lowercase())],
                ),
            }
        }
        Filter::Is(Structural::Unresolved) => (
            "EXISTS (SELECT 1 FROM links l WHERE l.file_id = f.id
              AND l.target_file_id IS NULL AND l.kind <> 'external' AND l.target_text <> '')"
                .into(),
            Vec::new(),
        ),
        Filter::Is(Structural::Orphan) => (
            "NOT EXISTS (SELECT 1 FROM links l WHERE l.target_file_id = f.id)".into(),
            Vec::new(),
        ),
        Filter::Is(Structural::Untagged) => (
            "NOT EXISTS (SELECT 1 FROM tags t WHERE t.file_id = f.id)".into(),
            Vec::new(),
        ),
        Filter::Is(Structural::DeadEnd) => (
            "NOT EXISTS (SELECT 1 FROM links l WHERE l.file_id = f.id AND l.kind <> 'external')"
                .into(),
            Vec::new(),
        ),
        Filter::Not(inner) => {
            let (sql, params) = filter_sql(inner);
            (format!("NOT ({sql})"), params)
        }
    }
}

fn escape_like(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(input: &str) -> Query {
        parse(input).unwrap()
    }

    #[test]
    fn bare_words_become_terms() {
        assert_eq!(
            q("machine learning").terms,
            vec![
                Term::Word("machine".into()),
                Term::Word("learning".into())
            ]
        );
    }

    #[test]
    fn a_quoted_string_is_one_phrase() {
        assert_eq!(
            q("\"machine learning\"").terms,
            vec![Term::Phrase("machine learning".into())]
        );
    }

    #[test]
    fn a_leading_dash_negates_a_term() {
        assert_eq!(
            q("-draft").terms,
            vec![Term::Not(Box::new(Term::Word("draft".into())))]
        );
    }

    #[test]
    fn a_dash_inside_a_word_is_part_of_the_word() {
        assert_eq!(q("well-known").terms, vec![Term::Word("well-known".into())]);
    }

    #[test]
    fn tag_filters_strip_a_leading_hash() {
        assert_eq!(q("tag:AI").filters, vec![Filter::Tag("AI".into())]);
        assert_eq!(q("tag:#AI").filters, vec![Filter::Tag("AI".into())]);
        assert_eq!(q("tag:AI/LLM").filters, vec![Filter::Tag("AI/LLM".into())]);
    }

    #[test]
    fn path_file_and_extension_filters_parse() {
        assert_eq!(q("path:Projects").filters, vec![Filter::Path("Projects".into())]);
        assert_eq!(q("file:readme").filters, vec![Filter::File("readme".into())]);
        assert_eq!(q("ext:.png").filters, vec![Filter::Extension("png".into())]);
    }

    #[test]
    fn an_unrecognised_prefix_is_treated_as_a_property() {
        assert_eq!(
            q("status:active").filters,
            vec![Filter::Property {
                key: "status".into(),
                comparison: Comparison::Equal,
                value: "active".into()
            }]
        );
    }

    #[test]
    fn numeric_comparisons_parse() {
        assert_eq!(
            q("rating>7").filters,
            vec![Filter::Property {
                key: "rating".into(),
                comparison: Comparison::Greater,
                value: "7".into()
            }]
        );
        assert_eq!(
            q("rating<=3").filters,
            vec![Filter::Property {
                key: "rating".into(),
                comparison: Comparison::LessOrEqual,
                value: "3".into()
            }]
        );
    }

    #[test]
    fn a_quoted_value_keeps_its_prefix() {
        assert_eq!(
            q("section:\"Design notes\"").filters,
            vec![Filter::Section("Design notes".into())]
        );
    }

    #[test]
    fn structural_filters_parse_and_reject_typos() {
        assert_eq!(
            q("is:orphan").filters,
            vec![Filter::Is(Structural::Orphan)]
        );
        let error = parse("is:nonsense").unwrap_err();
        assert_eq!(error.code(), "invalid_query");
        assert!(error.to_string().contains("unresolved"), "{error}");
    }

    #[test]
    fn a_negated_filter_is_wrapped() {
        assert_eq!(
            q("-tag:draft").filters,
            vec![Filter::Not(Box::new(Filter::Tag("draft".into())))]
        );
    }

    #[test]
    fn terms_and_filters_mix_freely() {
        let query = q("neural tag:AI path:Research \"exact phrase\" -old");
        assert_eq!(query.terms.len(), 3);
        assert_eq!(query.filters.len(), 2);
    }

    #[test]
    fn an_unterminated_quote_is_an_error_with_an_explanation() {
        let error = parse("\"never closed").unwrap_err();
        assert_eq!(error.code(), "invalid_query");
        assert!(error.to_string().contains("never closed"));
    }

    #[test]
    fn a_prefix_with_no_value_is_an_error() {
        assert!(parse("tag:").is_err());
    }

    #[test]
    fn an_empty_query_is_empty_rather_than_an_error() {
        let query = q("   ");
        assert!(query.is_empty());
    }

    #[test]
    fn fts_expressions_quote_everything_so_operators_are_literal() {
        assert_eq!(
            q("AND OR NOT").to_fts_expression().as_deref(),
            Some("\"AND\" AND \"OR\" AND \"NOT\"")
        );
        assert_eq!(
            q("say \"hi\"").to_fts_expression().as_deref(),
            Some("\"say\" AND \"hi\"")
        );
    }

    #[test]
    fn a_wildcard_character_is_searched_literally() {
        // Without quoting, FTS5 would read this as a prefix query.
        assert_eq!(q("a*").to_fts_expression().as_deref(), Some("\"a*\""));
    }

    #[test]
    fn a_filter_only_query_has_no_fts_expression() {
        assert_eq!(q("tag:AI").to_fts_expression(), None);
        assert!(!q("tag:AI").is_text_only());
        assert!(q("word").is_text_only());
    }

    #[test]
    fn negation_becomes_an_fts_not_operator() {
        assert_eq!(
            q("neural -draft").to_fts_expression().as_deref(),
            Some("\"neural\" NOT \"draft\"")
        );
    }

    #[test]
    fn like_wildcards_in_user_input_are_escaped() {
        let (_, params) = filter_sql(&Filter::Path("100%_done".into()));
        match &params[0] {
            rusqlite::types::Value::Text(text) => {
                assert!(text.contains("\\%"), "{text}");
                assert!(text.contains("\\_"), "{text}");
            }
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn a_numeric_property_value_produces_a_numeric_comparison() {
        let (sql, params) = filter_sql(&Filter::Property {
            key: "rating".into(),
            comparison: Comparison::Greater,
            value: "7".into(),
        });
        assert!(sql.contains("num_value >"), "{sql}");
        assert!(matches!(params[1], rusqlite::types::Value::Real(_)));
    }

    #[test]
    fn a_textual_property_value_produces_a_text_comparison() {
        let (sql, _) = filter_sql(&Filter::Property {
            key: "status".into(),
            comparison: Comparison::Equal,
            value: "active".into(),
        });
        assert!(sql.contains("text_fold ="), "{sql}");
    }
}
