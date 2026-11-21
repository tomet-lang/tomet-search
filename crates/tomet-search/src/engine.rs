//! Query evaluation and relevance ranking over extracted [`SearchUnit`]s.

use std::cmp::Ordering;

use tomet_hole::hole_engine::Capture;

use crate::compile::{CompiledNode, CompiledQuery, CompiledTerm};
use crate::extract::{SearchUnit, extract_units};
use crate::fuzzy::{fuzzy_match, normalize};
use crate::query::{FilterExpr, SearchHit, SearchOptions};

struct EvalOutcome {
    score: f32,
    captures: Vec<Capture>,
}

/// Evaluates `compiled` against `doc`, returning hits sorted by descending relevance.
pub fn evaluate_document(compiled: &CompiledQuery, doc: &tomet_ast::File) -> Vec<SearchHit> {
    let units = extract_units(doc);
    evaluate_units(compiled, &units)
}

/// Evaluates `compiled` against pre-extracted [`SearchUnit`]s.
pub fn evaluate_units(compiled: &CompiledQuery, units: &[SearchUnit]) -> Vec<SearchHit> {
    let mut hits = Vec::new();
    for unit in units {
        if let Some(outcome) = eval_node(&compiled.root, unit, &compiled.options) {
            let weight = compiled.options.scope_weights.weight_for(unit.scope);
            hits.push(SearchHit {
                span: unit.span,
                score: outcome.score * weight,
                tag: unit.tag.clone(),
                scope: unit.scope,
                captures: outcome.captures,
                matched_text: unit.text.clone(),
            });
        }
    }

    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.span.start.cmp(&b.span.start))
    });
    hits
}

fn eval_node(
    node: &CompiledNode,
    unit: &SearchUnit,
    options: &SearchOptions,
) -> Option<EvalOutcome> {
    match node {
        CompiledNode::And(children) => {
            let mut total_score = 0.0f32;
            let mut captures = Vec::new();
            for child in children {
                let outcome = eval_node(child, unit, options)?;
                total_score += outcome.score;
                captures.extend(outcome.captures);
            }
            Some(EvalOutcome {
                score: total_score,
                captures,
            })
        }
        CompiledNode::Or(children) => {
            let mut best_score: Option<f32> = None;
            let mut captures = Vec::new();
            for child in children {
                if let Some(outcome) = eval_node(child, unit, options) {
                    best_score = Some(best_score.map_or(outcome.score, |b| b.max(outcome.score)));
                    captures.extend(outcome.captures);
                }
            }
            best_score.map(|score| EvalOutcome { score, captures })
        }
        CompiledNode::Not(inner) => {
            if eval_node(inner, unit, options).is_none() {
                Some(EvalOutcome {
                    score: 0.0,
                    captures: Vec::new(),
                })
            } else {
                None
            }
        }
        CompiledNode::Filter(filter) => eval_filter(filter, unit).then_some(EvalOutcome {
            score: 1.0,
            captures: Vec::new(),
        }),
        CompiledNode::Term(term) => eval_term(term, unit, options),
    }
}

fn eval_filter(filter: &FilterExpr, unit: &SearchUnit) -> bool {
    match filter {
        FilterExpr::Tag(target) => unit
            .tag
            .as_deref()
            .is_some_and(|tag| tag.eq_ignore_ascii_case(target)),
        FilterExpr::Key(target) => unit.keys.iter().any(|key| key.eq_ignore_ascii_case(target)),
        FilterExpr::Id(target) => unit.id.as_deref().is_some_and(|id| id == target),
        FilterExpr::Scope(scope) => unit.scope == *scope,
    }
}

fn eval_term(
    term: &CompiledTerm,
    unit: &SearchUnit,
    options: &SearchOptions,
) -> Option<EvalOutcome> {
    match term {
        CompiledTerm::Text { normalized } => {
            let hay = normalize(&unit.text, options.case_sensitive);
            if hay.contains(normalized.as_str()) {
                Some(EvalOutcome {
                    score: 1.0,
                    captures: Vec::new(),
                })
            } else {
                None
            }
        }
        CompiledTerm::Phrase { normalized } => {
            let hay = normalize(&unit.text, options.case_sensitive);
            if hay.contains(normalized.as_str()) {
                Some(EvalOutcome {
                    score: 1.1,
                    captures: Vec::new(),
                })
            } else {
                None
            }
        }
        CompiledTerm::Fuzzy {
            normalized,
            max_distance,
        } => fuzzy_match(
            &unit.text,
            normalized,
            *max_distance,
            options.case_sensitive,
        )
        .map(|m| EvalOutcome {
            score: m.score,
            captures: Vec::new(),
        }),
        CompiledTerm::Hole { program } => program.find(&unit.text).map(|m| EvalOutcome {
            score: 1.05,
            captures: m.captures,
        }),
    }
}
