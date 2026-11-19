//! Query compilation into executable patterns and normalized terms.

use tomet_hole::hole_engine::Program;

use crate::fuzzy::normalize;
use crate::query::{CompileError, FilterExpr, Query, SearchOptions, TermExpr};

/// A compiled search query ready to evaluate across one or more documents.
#[derive(Debug, Clone)]
pub struct CompiledQuery {
    pub(crate) options: SearchOptions,
    pub(crate) root: CompiledNode,
}

impl CompiledQuery {
    /// Returns the [`SearchOptions`] this query was compiled with.
    pub fn options(&self) -> &SearchOptions {
        &self.options
    }

    /// Evaluates this compiled query against a parsed [`tomet_ast::File`].
    pub fn search(&self, doc: &tomet_ast::File) -> Vec<crate::SearchHit> {
        crate::engine::evaluate_document(self, doc)
    }
}

#[derive(Debug, Clone)]
pub(crate) enum CompiledNode {
    And(Vec<CompiledNode>),
    Or(Vec<CompiledNode>),
    Not(Box<CompiledNode>),
    Term(CompiledTerm),
    Filter(FilterExpr),
}

#[derive(Debug, Clone)]
pub(crate) enum CompiledTerm {
    Text {
        normalized: String,
    },
    Phrase {
        normalized: String,
    },
    Fuzzy {
        normalized: String,
        max_distance: Option<usize>,
    },
    Hole {
        program: Program,
    },
}

/// Compiles a [`Query`] AST into a [`CompiledQuery`].
pub fn compile_query(query: &Query, options: SearchOptions) -> Result<CompiledQuery, CompileError> {
    let root = compile_node(query, &options)?;
    Ok(CompiledQuery { options, root })
}

fn compile_node(node: &Query, options: &SearchOptions) -> Result<CompiledNode, CompileError> {
    match node {
        Query::And(children) => Ok(CompiledNode::And(
            children
                .iter()
                .map(|c| compile_node(c, options))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        Query::Or(children) => Ok(CompiledNode::Or(
            children
                .iter()
                .map(|c| compile_node(c, options))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        Query::Not(inner) => Ok(CompiledNode::Not(Box::new(compile_node(inner, options)?))),
        Query::Filter(filter) => Ok(CompiledNode::Filter(filter.clone())),
        Query::Term(term) => Ok(CompiledNode::Term(compile_term(term, options)?)),
    }
}

fn compile_term(term: &TermExpr, options: &SearchOptions) -> Result<CompiledTerm, CompileError> {
    match term {
        TermExpr::Text(s) => {
            let normalized = normalize(s, options.case_sensitive);
            if options.default_fuzzy {
                Ok(CompiledTerm::Fuzzy {
                    normalized,
                    max_distance: None,
                })
            } else {
                Ok(CompiledTerm::Text { normalized })
            }
        }
        TermExpr::Phrase(s) => Ok(CompiledTerm::Phrase {
            normalized: normalize(s, options.case_sensitive),
        }),
        TermExpr::Fuzzy { text, max_distance } => Ok(CompiledTerm::Fuzzy {
            normalized: normalize(text, options.case_sensitive),
            max_distance: *max_distance,
        }),
        TermExpr::Hole(src) => {
            let pattern = tomet_hole::compile_str_with_mode(src, options.hole_mode)
                .map_err(|e| CompileError::new(format!("invalid hole pattern '{src}': {e}")))?;
            Ok(CompiledTerm::Hole {
                program: Program::compile(&pattern),
            })
        }
    }
}
