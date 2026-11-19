//! Query AST, search options, and search hit types for `tomet-search`.

use std::fmt;

use tomet_hole::CompileMode;
use tomet_hole::hole_engine::Capture;
use tomet_span::Span;

/// A parsed search query AST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Query {
    /// All child queries must match.
    And(Vec<Query>),
    /// At least one child query must match.
    Or(Vec<Query>),
    /// Child query must not match.
    Not(Box<Query>),
    /// A text, phrase, fuzzy, or hole-pattern term.
    Term(TermExpr),
    /// A structural or vocabulary filter.
    Filter(FilterExpr),
}

impl Query {
    /// Parses a one-liner search query string into a [`Query`] AST.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        crate::parser::parse_query(input)
    }

    /// Compiles this query into a [`crate::CompiledQuery`] ready for repeated evaluation.
    pub fn compile(&self, options: SearchOptions) -> Result<crate::CompiledQuery, CompileError> {
        crate::compile::compile_query(self, options)
    }
}

/// A leaf search term matching against extracted text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TermExpr {
    /// A bare word or token.
    Text(String),
    /// An exact quoted phrase.
    Phrase(String),
    /// A typo-tolerant fuzzy term with an optional explicit max edit distance.
    Fuzzy {
        text: String,
        max_distance: Option<usize>,
    },
    /// A `tomet-hole` pattern string containing `<?...>` matchers.
    Hole(String),
}

/// A structural or vocabulary-level filter predicate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterExpr {
    /// Matches an element or declaration tag name (case-insensitive).
    Tag(String),
    /// Matches an element or declaration that defines the given property key.
    Key(String),
    /// Matches a node with the given `#id`.
    Id(String),
    /// Restricts matching to a semantic [`ScopeKind`].
    Scope(ScopeKind),
}

/// Semantic scope of an extracted [`crate::SearchUnit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScopeKind {
    Heading,
    Meta,
    Body,
    Code,
    Link,
    List,
    Declaration,
}

impl ScopeKind {
    /// Parses a scope name (case-insensitive) for `in:<scope>` filters.
    pub fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "heading" | "title" | "section" => Some(Self::Heading),
            "meta" => Some(Self::Meta),
            "body" | "paragraph" | "text" => Some(Self::Body),
            "code" => Some(Self::Code),
            "link" => Some(Self::Link),
            "list" => Some(Self::List),
            "decl" | "declaration" => Some(Self::Declaration),
            _ => None,
        }
    }

    /// Returns the canonical lowercase name of this scope.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Heading => "heading",
            Self::Meta => "meta",
            Self::Body => "body",
            Self::Code => "code",
            Self::Link => "link",
            Self::List => "list",
            Self::Declaration => "declaration",
        }
    }
}

/// Per-scope score multipliers used during ranking.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScopeWeights {
    pub heading: f32,
    pub meta: f32,
    pub body: f32,
    pub code: f32,
    pub link: f32,
    pub list: f32,
    pub declaration: f32,
}

impl Default for ScopeWeights {
    fn default() -> Self {
        Self {
            heading: 2.0,
            meta: 1.5,
            body: 1.0,
            code: 0.8,
            link: 1.2,
            list: 1.0,
            declaration: 1.3,
        }
    }
}

impl ScopeWeights {
    /// Returns the configured multiplier for `scope`.
    pub fn weight_for(self, scope: ScopeKind) -> f32 {
        match scope {
            ScopeKind::Heading => self.heading,
            ScopeKind::Meta => self.meta,
            ScopeKind::Body => self.body,
            ScopeKind::Code => self.code,
            ScopeKind::Link => self.link,
            ScopeKind::List => self.list,
            ScopeKind::Declaration => self.declaration,
        }
    }
}

/// Configuration controlling query compilation, matching, and ranking.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchOptions {
    /// Whether text and phrase matching is case-sensitive.
    pub case_sensitive: bool,
    /// When true, bare [`TermExpr::Text`] terms also allow typo-tolerant fuzzy matches.
    pub default_fuzzy: bool,
    /// Compilation mode passed to `tomet-hole` when compiling [`TermExpr::Hole`].
    pub hole_mode: CompileMode,
    /// Score multipliers per semantic scope.
    pub scope_weights: ScopeWeights,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            case_sensitive: false,
            default_fuzzy: false,
            hole_mode: CompileMode::Text,
            scope_weights: ScopeWeights::default(),
        }
    }
}

/// A ranked search result within a Tomet document.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    /// Source span of the matched AST unit.
    pub span: Span,
    /// Relevance score (higher is more relevant).
    pub score: f32,
    /// Semantic or element tag name of the matched unit, if applicable.
    pub tag: Option<String>,
    /// Semantic scope of the matched unit.
    pub scope: ScopeKind,
    /// Captures produced by `tomet-hole` patterns, if any.
    pub captures: Vec<Capture>,
    /// Flattened text of the matched unit for snippet/preview rendering.
    pub matched_text: String,
}

/// Error returned when a search query string cannot be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub offset: usize,
    pub message: String,
}

impl ParseError {
    pub(crate) fn new(offset: usize, message: impl Into<String>) -> Self {
        Self {
            offset,
            message: message.into(),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "query parse error at byte {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Error returned when a [`Query`] cannot be compiled (e.g. an invalid hole pattern).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileError {
    pub message: String,
}

impl CompileError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "query compile error: {}", self.message)
    }
}

impl std::error::Error for CompileError {}

/// Combined error returned by one-shot search helpers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchError {
    Parse(ParseError),
    Compile(CompileError),
}

impl fmt::Display for SearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(err) => err.fmt(f),
            Self::Compile(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for SearchError {}

impl From<ParseError> for SearchError {
    fn from(err: ParseError) -> Self {
        Self::Parse(err)
    }
}

impl From<CompileError> for SearchError {
    fn from(err: CompileError) -> Self {
        Self::Compile(err)
    }
}
