//! One-liner search query parser.
//!
//! Grammar (precedence from lowest to highest):
//! - `or_expr  := and_expr ( "OR" and_expr )*`
//! - `and_expr := not_expr ( ("AND")? not_expr )*`
//! - `not_expr := ( "NOT" | "-" ) not_expr | primary`
//! - `primary  := "(" or_expr ")" | filter | term`

use crate::query::{FilterExpr, ParseError, Query, ScopeKind, TermExpr};

/// Parses a one-liner search query string into a [`Query`] AST.
pub fn parse_query(input: &str) -> Result<Query, ParseError> {
    let mut parser = Parser::new(input);
    parser.skip_ws();
    if parser.is_eof() {
        return Err(ParseError::new(0, "empty search query"));
    }
    let expr = parser.parse_or()?;
    parser.skip_ws();
    if !parser.is_eof() {
        return Err(ParseError::new(
            parser.pos,
            format!(
                "unexpected token '{}'",
                parser.peek_char().unwrap_or_default()
            ),
        ));
    }
    Ok(expr)
}

struct Parser<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.src.len()
    }

    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    fn peek_char(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn skip_ws(&mut self) {
        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace() {
                self.pos += ch.len_utf8();
            } else {
                break;
            }
        }
    }

    fn starts_with_keyword(&self, kw: &str) -> bool {
        if !self.rest().starts_with(kw) {
            return false;
        }
        let after = &self.rest()[kw.len()..];
        after.is_empty()
            || after
                .chars()
                .next()
                .is_some_and(|c| c.is_whitespace() || c == '(' || c == ')')
    }

    fn consume_keyword(&mut self, kw: &str) -> bool {
        if self.starts_with_keyword(kw) {
            self.pos += kw.len();
            true
        } else {
            false
        }
    }

    fn parse_or(&mut self) -> Result<Query, ParseError> {
        let mut branches = vec![self.parse_and()?];
        loop {
            self.skip_ws();
            if self.consume_keyword("OR") {
                self.skip_ws();
                if self.is_eof() || self.peek_char() == Some(')') {
                    return Err(ParseError::new(self.pos, "expected expression after 'OR'"));
                }
                branches.push(self.parse_and()?);
            } else {
                break;
            }
        }
        Ok(if branches.len() == 1 {
            branches.pop().expect("single branch")
        } else {
            Query::Or(branches)
        })
    }

    fn parse_and(&mut self) -> Result<Query, ParseError> {
        let mut factors = vec![self.parse_not()?];
        loop {
            self.skip_ws();
            if self.is_eof() || self.peek_char() == Some(')') || self.starts_with_keyword("OR") {
                break;
            }
            if self.consume_keyword("AND") {
                self.skip_ws();
                if self.is_eof() || self.peek_char() == Some(')') {
                    return Err(ParseError::new(self.pos, "expected expression after 'AND'"));
                }
            }
            factors.push(self.parse_not()?);
        }
        Ok(if factors.len() == 1 {
            factors.pop().expect("single factor")
        } else {
            Query::And(factors)
        })
    }

    fn parse_not(&mut self) -> Result<Query, ParseError> {
        self.skip_ws();
        if self.consume_keyword("NOT") {
            let start = self.pos;
            self.skip_ws();
            if self.is_eof() || self.peek_char() == Some(')') {
                return Err(ParseError::new(start, "expected expression after 'NOT'"));
            }
            return Ok(Query::Not(Box::new(self.parse_not()?)));
        }
        if self.peek_char() == Some('-') {
            let start = self.pos;
            self.pos += 1;
            if self.is_eof() || self.peek_char().is_some_and(char::is_whitespace) {
                return Err(ParseError::new(start, "expected term after '-'"));
            }
            return Ok(Query::Not(Box::new(self.parse_not()?)));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<Query, ParseError> {
        self.skip_ws();
        let start = self.pos;
        match self.peek_char() {
            Some('(') => {
                self.pos += 1;
                self.skip_ws();
                if self.peek_char() == Some(')') {
                    return Err(ParseError::new(start, "empty parenthesized group"));
                }
                let inner = self.parse_or()?;
                self.skip_ws();
                if self.peek_char() != Some(')') {
                    return Err(ParseError::new(start, "unclosed '('"));
                }
                self.pos += 1;
                Ok(inner)
            }
            Some(')') => Err(ParseError::new(start, "unexpected ')'")),
            Some('"') => {
                let phrase = self.parse_quoted()?;
                Ok(Query::Term(TermExpr::Phrase(phrase)))
            }
            Some('~') => self.parse_prefix_fuzzy(),
            Some('#') => self.parse_id_filter(),
            Some(_) => self.parse_prefixed_or_bare(),
            None => Err(ParseError::new(start, "unexpected end of query")),
        }
    }

    fn parse_quoted(&mut self) -> Result<String, ParseError> {
        let start = self.pos;
        self.pos += 1; // opening '"'
        let mut out = String::new();
        while let Some(ch) = self.peek_char() {
            self.pos += ch.len_utf8();
            match ch {
                '"' => return Ok(out),
                '\\' => {
                    let Some(esc) = self.peek_char() else {
                        return Err(ParseError::new(
                            start,
                            "unterminated escape in quoted string",
                        ));
                    };
                    self.pos += esc.len_utf8();
                    match esc {
                        'n' => out.push('\n'),
                        't' => out.push('\t'),
                        'r' => out.push('\r'),
                        other => out.push(other),
                    }
                }
                other => out.push(other),
            }
        }
        Err(ParseError::new(start, "unclosed '\"'"))
    }

    fn parse_prefix_fuzzy(&mut self) -> Result<Query, ParseError> {
        let start = self.pos;
        self.pos += 1; // '~'
        let word = self.scan_bare_token();
        if word.is_empty() {
            return Err(ParseError::new(start, "expected term after '~'"));
        }
        Ok(Query::Term(TermExpr::Fuzzy {
            text: word,
            max_distance: None,
        }))
    }

    fn parse_id_filter(&mut self) -> Result<Query, ParseError> {
        let start = self.pos;
        self.pos += 1; // '#'
        let id = self.scan_value()?;
        if id.is_empty() {
            return Err(ParseError::new(start, "expected identifier after '#'"));
        }
        Ok(Query::Filter(FilterExpr::Id(id)))
    }

    fn parse_prefixed_or_bare(&mut self) -> Result<Query, ParseError> {
        let start = self.pos;

        // Check for explicit `prefix:value` before scanning a full bare token so that
        // `hole:"@meta(<?(k)>: <?>)"` or `tag:"my tag"` parses cleanly.
        for (prefix, kind) in [
            ("hole:", PrefixKind::Hole),
            ("pat:", PrefixKind::Hole),
            ("tag:", PrefixKind::Tag),
            ("key:", PrefixKind::Key),
            ("in:", PrefixKind::Scope),
        ] {
            if self.rest().starts_with(prefix) {
                self.pos += prefix.len();
                let val_start = self.pos;
                let value = self.scan_value()?;
                if value.is_empty() {
                    return Err(ParseError::new(
                        val_start,
                        format!("expected value after '{prefix}'"),
                    ));
                }
                return match kind {
                    PrefixKind::Hole => Ok(Query::Term(TermExpr::Hole(value))),
                    PrefixKind::Tag => Ok(Query::Filter(FilterExpr::Tag(value))),
                    PrefixKind::Key => Ok(Query::Filter(FilterExpr::Key(value))),
                    PrefixKind::Scope => {
                        let scope = ScopeKind::parse(&value).ok_or_else(|| {
                            ParseError::new(
                                val_start,
                                format!(
                                    "unknown scope '{value}' (expected heading, meta, body, code, link, list, or declaration)"
                                ),
                            )
                        })?;
                        Ok(Query::Filter(FilterExpr::Scope(scope)))
                    }
                };
            }
        }

        let token = self.scan_bare_token();
        if token.is_empty() {
            return Err(ParseError::new(start, "expected search term"));
        }

        if token.contains("<?") {
            return Ok(Query::Term(TermExpr::Hole(token)));
        }

        if let Some((word, dist)) = split_suffix_fuzzy(&token) {
            return Ok(Query::Term(TermExpr::Fuzzy {
                text: word.to_string(),
                max_distance: dist,
            }));
        }

        Ok(Query::Term(TermExpr::Text(token)))
    }

    fn scan_value(&mut self) -> Result<String, ParseError> {
        if self.peek_char() == Some('"') {
            self.parse_quoted()
        } else {
            Ok(self.scan_bare_token())
        }
    }

    /// Scans a bare token up to whitespace or `)`.
    ///
    /// Keeps `<?...>` hole expressions (including spaces inside `<?(...)>`) intact
    /// as part of the same token.
    fn scan_bare_token(&mut self) -> String {
        let start = self.pos;
        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace() || ch == ')' {
                break;
            }
            if self.rest().starts_with("<?") {
                self.scan_hole_span();
                continue;
            }
            self.pos += ch.len_utf8();
        }
        self.src[start..self.pos].to_string()
    }

    fn scan_hole_span(&mut self) {
        self.pos += 2; // "<?"
        let mut paren_depth = 0usize;
        while let Some(ch) = self.peek_char() {
            self.pos += ch.len_utf8();
            match ch {
                '(' => paren_depth += 1,
                ')' => paren_depth = paren_depth.saturating_sub(1),
                '>' if paren_depth == 0 => break,
                _ => {}
            }
        }
    }
}

enum PrefixKind {
    Hole,
    Tag,
    Key,
    Scope,
}

fn split_suffix_fuzzy(token: &str) -> Option<(&str, Option<usize>)> {
    let tilde = token.rfind('~')?;
    let (word, suffix) = token.split_at(tilde);
    if word.is_empty() {
        return None;
    }
    let dist_str = &suffix[1..];
    if dist_str.is_empty() {
        return Some((word, None));
    }
    if let Ok(dist) = dist_str.parse::<usize>() {
        return Some((word, Some(dist)));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_implicit_and_and_or_precedence() {
        let q = parse_query("alpha beta OR gamma").unwrap();
        assert_eq!(
            q,
            Query::Or(vec![
                Query::And(vec![
                    Query::Term(TermExpr::Text("alpha".into())),
                    Query::Term(TermExpr::Text("beta".into())),
                ]),
                Query::Term(TermExpr::Text("gamma".into())),
            ])
        );
    }

    #[test]
    fn parses_filters_negation_and_groups() {
        let q = parse_query("(tag:heading OR tag:meta) -in:code #intro key:author").unwrap();
        assert_eq!(
            q,
            Query::And(vec![
                Query::Or(vec![
                    Query::Filter(FilterExpr::Tag("heading".into())),
                    Query::Filter(FilterExpr::Tag("meta".into())),
                ]),
                Query::Not(Box::new(Query::Filter(FilterExpr::Scope(ScopeKind::Code)))),
                Query::Filter(FilterExpr::Id("intro".into())),
                Query::Filter(FilterExpr::Key("author".into())),
            ])
        );
    }

    #[test]
    fn parses_fuzzy_and_hole_terms() {
        let q = parse_query("~srach tomet~2 \"exact phrase\" fn_<?(name)> hole:\"@meta(<?(k)>)\"")
            .unwrap();
        assert_eq!(
            q,
            Query::And(vec![
                Query::Term(TermExpr::Fuzzy {
                    text: "srach".into(),
                    max_distance: None,
                }),
                Query::Term(TermExpr::Fuzzy {
                    text: "tomet".into(),
                    max_distance: Some(2),
                }),
                Query::Term(TermExpr::Phrase("exact phrase".into())),
                Query::Term(TermExpr::Hole("fn_<?(name)>".into())),
                Query::Term(TermExpr::Hole("@meta(<?(k)>)".into())),
            ])
        );
    }

    #[test]
    fn keeps_spaces_inside_bare_hole_expression() {
        let q = parse_query("<?(alpha, beta)>").unwrap();
        assert_eq!(q, Query::Term(TermExpr::Hole("<?(alpha, beta)>".into())));
    }

    #[test]
    fn rejects_invalid_queries() {
        assert!(parse_query("   ").is_err());
        assert!(parse_query("(a OR b").is_err());
        assert!(parse_query("\"unclosed").is_err());
        assert!(parse_query("in:unknown").is_err());
        assert!(parse_query("- ").is_err());
    }
}
