//! AST projection from [`tomet_ast::File`] into searchable [`SearchUnit`]s.

use tomet_ast::{
    Block, Data, Declaration, Element, File, Hole, Inline, List, ListItem, Tuple, Value,
};
use tomet_scalar::Scalar;
use tomet_semantics::{ElementKind, classify_declaration_lenient, classify_std_lenient};
use tomet_span::Span;

use crate::query::ScopeKind;

/// A flattened, searchable unit extracted from a Tomet AST node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchUnit {
    /// Source span of the AST node.
    pub span: Span,
    /// Semantic scope of this unit.
    pub scope: ScopeKind,
    /// Element or declaration tag name (e.g. `"heading"`, `"meta"`, `"link"`).
    pub tag: Option<String>,
    /// Property/attribute keys defined on this node.
    pub keys: Vec<String>,
    /// Explicit `#id` attached to this node, if any.
    pub id: Option<String>,
    /// Flattened searchable text of this unit.
    pub text: String,
}

/// Extracts all searchable units from `doc` in document order.
pub fn extract_units(doc: &File) -> Vec<SearchUnit> {
    let mut units = Vec::new();
    for decl in &doc.header {
        extract_declaration(decl, &mut units);
    }
    extract_blocks(&doc.body, &mut units);
    units
}

fn extract_declaration(decl: &Declaration, out: &mut Vec<SearchUnit>) {
    let kind = classify_declaration_lenient(decl);
    let tag = kind.as_str().to_string();
    let scope = if tag.eq_ignore_ascii_case("meta") {
        ScopeKind::Meta
    } else {
        ScopeKind::Declaration
    };

    let mut text = String::new();
    push_data_text(&decl.data, &mut text);
    if let Some(blocks) = &decl.content {
        push_blocks_text(blocks, &mut text);
    }

    out.push(SearchUnit {
        span: decl.span,
        scope,
        tag: Some(tag),
        keys: collect_keys(&decl.data),
        id: decl.id.as_ref().map(|id| id.0.clone()),
        text: text.trim().to_string(),
    });

    if let Some(blocks) = &decl.content {
        extract_blocks(blocks, out);
    }
}

fn extract_blocks(blocks: &[Block], out: &mut Vec<SearchUnit>) {
    for block in blocks {
        match block {
            Block::Section(sec) => {
                let mut text = String::new();
                push_inlines_text(&sec.title, &mut text);
                push_data_text(&sec.data, &mut text);
                out.push(SearchUnit {
                    span: sec.span,
                    scope: ScopeKind::Heading,
                    tag: Some("heading".to_string()),
                    keys: collect_keys(&sec.data),
                    id: sec.id.as_ref().map(|id| id.0.clone()),
                    text: text.trim().to_string(),
                });
                extract_blocks(&sec.blocks, out);
            }
            Block::Paragraph(para) => {
                let mut text = String::new();
                push_inlines_text(&para.content, &mut text);
                out.push(SearchUnit {
                    span: para.span,
                    scope: ScopeKind::Body,
                    tag: None,
                    keys: Vec::new(),
                    id: None,
                    text: text.trim().to_string(),
                });
                extract_inline_elements(&para.content, out);
            }
            Block::Element(el) => extract_element(el, out),
            Block::List(list) => extract_list(list, out),
            Block::Hole(hole) => out.push(SearchUnit {
                span: hole.span,
                scope: ScopeKind::Body,
                tag: None,
                keys: Vec::new(),
                id: None,
                text: hole_to_text(hole),
            }),
            Block::TypeSeat(seat) => out.push(SearchUnit {
                span: seat.span,
                scope: ScopeKind::Body,
                tag: None,
                keys: Vec::new(),
                id: None,
                text: seat.expr.to_string(),
            }),
            Block::Interp(interp) => out.push(SearchUnit {
                span: interp.span,
                scope: ScopeKind::Body,
                tag: None,
                keys: Vec::new(),
                id: None,
                text: interp.expr.to_string(),
            }),
        }
    }
}

fn extract_element(el: &Element, out: &mut Vec<SearchUnit>) {
    let kind = classify_std_lenient(el.as_view());
    let tag = kind.as_str().to_string();
    let scope = scope_for_element(&kind, &tag);

    let mut text = String::new();
    push_data_text(&el.data, &mut text);
    if let Some(blocks) = &el.content {
        push_blocks_text(blocks, &mut text);
    }

    out.push(SearchUnit {
        span: el.span,
        scope,
        tag: Some(tag),
        keys: collect_keys(&el.data),
        id: el.id.as_ref().map(|id| id.0.clone()),
        text: text.trim().to_string(),
    });

    if scope != ScopeKind::Code
        && let Some(blocks) = &el.content
    {
        extract_blocks(blocks, out);
    }
}

fn extract_inline_elements(inlines: &[Inline], out: &mut Vec<SearchUnit>) {
    for inline in inlines {
        if let Inline::Element(el) = inline {
            let kind = classify_std_lenient(el.as_view());
            let has_metadata = el.id.is_some()
                || !collect_keys(&el.data).is_empty()
                || matches!(
                    kind,
                    ElementKind::Link
                        | ElementKind::File
                        | ElementKind::Dir
                        | ElementKind::Draft
                        | ElementKind::Fixme
                        | ElementKind::Conflict
                        | ElementKind::Embed
                )
                || kind.as_str().eq_ignore_ascii_case("code");
            if has_metadata {
                extract_element(el, out);
            } else if let Some(blocks) = &el.content {
                for block in blocks {
                    if let Block::Paragraph(p) = block {
                        extract_inline_elements(&p.content, out);
                    }
                }
            }
        }
    }
}

fn extract_list(list: &List, out: &mut Vec<SearchUnit>) {
    for item in &list.items {
        extract_list_item(item, out);
    }
}

fn extract_list_item(item: &ListItem, out: &mut Vec<SearchUnit>) {
    let tag = item
        .as_element_view()
        .map(|view| classify_std_lenient(view).as_str().to_string());

    let mut text = String::new();
    push_data_text(&item.data, &mut text);
    push_blocks_text(&item.content, &mut text);

    out.push(SearchUnit {
        span: item.span,
        scope: ScopeKind::List,
        tag,
        keys: collect_keys(&item.data),
        id: item.id.as_ref().map(|id| id.0.clone()),
        text: text.trim().to_string(),
    });

    if let Some(sublist) = &item.sublist {
        extract_list(sublist, out);
    }
}

fn scope_for_element(kind: &ElementKind, tag: &str) -> ScopeKind {
    if tag.eq_ignore_ascii_case("meta") {
        return ScopeKind::Meta;
    }
    if tag.eq_ignore_ascii_case("code") || tag.eq_ignore_ascii_case("codeblock") {
        return ScopeKind::Code;
    }
    match kind {
        ElementKind::Link => ScopeKind::Link,
        ElementKind::Declaration(_) => ScopeKind::Declaration,
        _ => ScopeKind::Body,
    }
}

fn collect_keys(data: &Data) -> Vec<String> {
    let mut keys = Vec::new();
    if let Some(args) = &data.args {
        collect_tuple_keys(args, &mut keys);
    }
    if let Some(val) = &data.value {
        collect_tuple_keys(&val.as_data(), &mut keys);
    }
    keys
}

fn collect_tuple_keys(tuple: &Tuple, keys: &mut Vec<String>) {
    for entry in tuple.entries() {
        if let Some(name) = entry.name() {
            let name_str = name.to_string();
            if !keys.contains(&name_str) {
                keys.push(name_str);
            }
        }
    }
}

fn hole_to_text(hole: &Hole) -> String {
    let mut text = String::new();
    if let Some(matcher) = &hole.matcher {
        push_word(&mut text, matcher.name.as_str());
    }
    push_data_text(&hole.data, &mut text);
    if let Some(blocks) = &hole.content {
        push_blocks_text(blocks, &mut text);
    }
    text
}

fn push_word(out: &mut String, text: &str) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    if !out.is_empty() && !out.ends_with(char::is_whitespace) {
        out.push(' ');
    }
    out.push_str(trimmed);
}

fn push_blocks_text(blocks: &[Block], out: &mut String) {
    for block in blocks {
        match block {
            Block::Paragraph(p) => push_inlines_text(&p.content, out),
            Block::Section(s) => {
                push_inlines_text(&s.title, out);
                push_data_text(&s.data, out);
                push_blocks_text(&s.blocks, out);
            }
            Block::Element(e) => {
                push_data_text(&e.data, out);
                if let Some(content) = &e.content {
                    push_blocks_text(content, out);
                }
            }
            Block::List(list) => {
                for item in &list.items {
                    push_data_text(&item.data, out);
                    push_blocks_text(&item.content, out);
                }
            }
            Block::Hole(h) => push_word(out, &hole_to_text(h)),
            Block::TypeSeat(s) => push_word(out, &s.expr.to_string()),
            Block::Interp(i) => push_word(out, &i.expr.to_string()),
        }
    }
}

fn push_inlines_text(inlines: &[Inline], out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(t) => out.push_str(&t.value),
            Inline::Raw(r) => out.push_str(&r.value),
            Inline::SoftBreak(_) | Inline::LineBreak(_) => {
                if !out.is_empty() && !out.ends_with(char::is_whitespace) {
                    out.push(' ');
                }
            }
            Inline::Element(el) => {
                if let Some(content) = &el.content {
                    push_blocks_text(content, out);
                } else {
                    push_data_text(&el.data, out);
                }
            }
            Inline::Hole(h) => push_word(out, &hole_to_text(h)),
            Inline::TypeSeat(s) => out.push_str(&s.expr.to_string()),
            Inline::Interp(i) => out.push_str(&i.expr.to_string()),
        }
    }
}

fn push_data_text(data: &Data, out: &mut String) {
    if let Some(args) = &data.args {
        push_tuple_text(args, out);
    }
    if let Some(val) = &data.value {
        push_tuple_text(&val.as_data(), out);
    }
}

fn push_tuple_text(tuple: &Tuple, out: &mut String) {
    for entry in tuple.entries() {
        if let Some(name) = entry.name() {
            push_word(out, name);
        }
        push_value_text(entry.value(), out);
    }
}

fn push_value_text(value: &Value, out: &mut String) {
    match value {
        Value::Scalar(scalar) => match scalar {
            Scalar::Text(s) | Scalar::Number(s) => push_word(out, s),
            Scalar::Bool(b) => push_word(out, if *b { "true" } else { "false" }),
            Scalar::Int(i) => push_word(out, &i.to_string()),
            Scalar::Float(f) => push_word(out, &f.to_string()),
            Scalar::None | Scalar::Auto => {}
        },
        Value::Tuple(tuple) => push_tuple_text(tuple, out),
        Value::Call(name, args) => {
            push_word(out, name);
            push_tuple_text(args, out);
        }
        Value::Interp(i) => push_word(out, &i.expr.to_string()),
        Value::TypeSeat(s) => push_word(out, &s.expr.to_string()),
        Value::Hole(h) => push_word(out, &hole_to_text(h)),
        Value::Element(el) => {
            push_data_text(&el.data, out);
            if let Some(content) = &el.content {
                push_blocks_text(content, out);
            }
        }
        Value::Blocks(blocks) => push_blocks_text(blocks, out),
    }
}
