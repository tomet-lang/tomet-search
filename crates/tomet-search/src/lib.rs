//! Tomet-aware search engine.
//!
//! Provides query parsing, pattern-based (`tomet-hole`) matching,
//! typo-tolerant fuzzy search, and vocabulary-driven ranking over Tomet documents.

pub mod compile;
pub mod engine;
pub mod extract;
pub mod fuzzy;
pub mod parser;
pub mod query;

pub use compile::CompiledQuery;
pub use extract::{SearchUnit, extract_units};
pub use fuzzy::FuzzyMatch;
pub use query::{
    CompileError, FilterExpr, ParseError, Query, ScopeKind, ScopeWeights, SearchError, SearchHit,
    SearchOptions, TermExpr,
};

/// Parses, compiles, and runs `query_str` against `doc` with default [`SearchOptions`].
pub fn search_str(doc: &tomet_ast::File, query_str: &str) -> Result<Vec<SearchHit>, SearchError> {
    search_str_with(doc, query_str, SearchOptions::default())
}

/// Parses, compiles, and runs `query_str` against `doc` with custom `options`.
pub fn search_str_with(
    doc: &tomet_ast::File,
    query_str: &str,
    options: SearchOptions,
) -> Result<Vec<SearchHit>, SearchError> {
    let query = Query::parse(query_str)?;
    let compiled = query.compile(options)?;
    Ok(compiled.search(doc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast_parser::parse_file;

    const SAMPLE_DOC: &str = "\
::meta(author: \"Alice\", status: \"draft\")

=[ Introduction ]#(intro)

Tomet is a structured document language with hole patterns.

==[ Search Architecture ]

We use @link(target: \"https://example.com\")[hole-engine] for pattern matching.

@code(lang: \"rust\")[fn main() { println!(\"hello\"); }]
";

    #[test]
    fn ranks_heading_higher_than_body() {
        let doc = parse_file(SAMPLE_DOC).unwrap();
        let hits = search_str(&doc, "Search").unwrap();
        assert!(!hits.is_empty());
        assert_eq!(hits[0].scope, ScopeKind::Heading);
        assert_eq!(hits[0].tag.as_deref(), Some("heading"));
    }

    #[test]
    fn filters_by_tag_key_id_and_scope() {
        let doc = parse_file(SAMPLE_DOC).unwrap();

        let meta_hits = search_str(&doc, "tag:meta key:author Alice").unwrap();
        assert_eq!(meta_hits.len(), 1);
        assert_eq!(meta_hits[0].scope, ScopeKind::Meta);

        let id_hits = search_str(&doc, "#intro").unwrap();
        assert_eq!(id_hits.len(), 1);
        assert_eq!(id_hits[0].scope, ScopeKind::Heading);

        let link_hits = search_str(&doc, "in:link hole-engine").unwrap();
        assert_eq!(link_hits.len(), 1);
        assert_eq!(link_hits[0].tag.as_deref(), Some("link"));
    }

    #[test]
    fn supports_typo_tolerant_fuzzy_search() {
        let doc = parse_file(SAMPLE_DOC).unwrap();
        let hits = search_str(&doc, "~structred").unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].matched_text.contains("structured"));
    }

    #[test]
    fn supports_hole_patterns_and_captures() {
        let doc = parse_file(SAMPLE_DOC).unwrap();
        let hits = search_str(&doc, "hole:\"fn <?(name)>()\"").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].scope, ScopeKind::Code);
        let cap = hits[0]
            .captures
            .iter()
            .find_map(|c| c.get("name"))
            .expect("captured function name");
        assert_eq!(cap.text, "main");
    }

    #[test]
    fn supports_boolean_negation_and_phrases() {
        let doc = parse_file(SAMPLE_DOC).unwrap();
        let hits = search_str(&doc, "\"pattern matching\" -in:code").unwrap();
        assert!(!hits.is_empty());
        assert!(hits.iter().all(|h| h.scope != ScopeKind::Code));
    }
}
