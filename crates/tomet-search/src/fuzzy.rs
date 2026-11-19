//! Typo-tolerant fuzzy matching using Optimal String Alignment (Damerau-Levenshtein).

/// Result of a successful fuzzy match.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FuzzyMatch {
    /// Edit distance between the query term and the best matching token.
    pub distance: usize,
    /// Normalized match score in `(0.0, 1.0]`.
    pub score: f32,
}

/// Normalizes text for comparison according to `case_sensitive`.
pub fn normalize(text: &str, case_sensitive: bool) -> String {
    if case_sensitive {
        text.to_string()
    } else {
        text.to_lowercase()
    }
}

/// Returns the default maximum edit distance for a term of `char_len` characters.
pub fn default_max_distance(char_len: usize) -> usize {
    match char_len {
        0..=2 => 0,
        3..=5 => 1,
        _ => 2,
    }
}

/// Matches a pre-normalized `needle` against `haystack` with typo tolerance.
///
/// Exact substring matches always succeed with `distance == 0` and `score == 1.0`.
/// Otherwise, `haystack` is tokenized on whitespace and punctuation, and the
/// token with the smallest Damerau-Levenshtein distance within `limit` wins.
pub fn fuzzy_match(
    haystack: &str,
    normalized_needle: &str,
    max_distance: Option<usize>,
    case_sensitive: bool,
) -> Option<FuzzyMatch> {
    if normalized_needle.is_empty() {
        return None;
    }

    let norm_hay = normalize(haystack, case_sensitive);
    if norm_hay.contains(normalized_needle) {
        return Some(FuzzyMatch {
            distance: 0,
            score: 1.0,
        });
    }

    let needle_chars: Vec<char> = normalized_needle.chars().collect();
    let limit = max_distance.unwrap_or_else(|| default_max_distance(needle_chars.len()));
    if limit == 0 {
        return None;
    }

    let mut best_distance: Option<usize> = None;
    for token in tokenize(&norm_hay) {
        let token_chars: Vec<char> = token.chars().collect();
        if token_chars.len().abs_diff(needle_chars.len()) > limit {
            continue;
        }
        let dist = damerau_levenshtein(&needle_chars, &token_chars);
        if dist <= limit && best_distance.is_none_or(|best| dist < best) {
            best_distance = Some(dist);
            if dist == 1 {
                // Distance 0 was already handled by `contains`, so 1 is optimal here.
                break;
            }
        }
    }

    best_distance.map(|distance| FuzzyMatch {
        distance,
        score: 1.0 / (1.0 + distance as f32),
    })
}

fn tokenize(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| c.is_whitespace() || is_token_separator(c))
        .filter(|tok| !tok.is_empty())
}

fn is_token_separator(c: char) -> bool {
    matches!(
        c,
        '.' | ','
            | ':'
            | ';'
            | '!'
            | '?'
            | '('
            | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | '"'
            | '\''
            | '`'
            | '/'
            | '\\'
            | '<'
            | '>'
            | '|'
            | '='
            | '+'
            | '*'
    )
}

/// Computes the Optimal String Alignment (restricted Damerau-Levenshtein) distance.
pub fn damerau_levenshtein(a: &[char], b: &[char]) -> usize {
    let n = a.len();
    let m = b.len();
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }

    let mut prev2 = vec![0usize; m + 1];
    let mut prev1: Vec<usize> = (0..=m).collect();
    let mut curr = vec![0usize; m + 1];

    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (prev1[j] + 1)
                .min(curr[j - 1] + 1)
                .min(prev1[j - 1] + cost);

            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(prev2[j - 2] + 1);
            }
            curr[j] = best;
        }
        std::mem::swap(&mut prev2, &mut prev1);
        std::mem::swap(&mut prev1, &mut curr);
    }

    prev1[m]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_substring_scores_highest() {
        let m = fuzzy_match("Hello Tomet Search", "tomet", None, false).unwrap();
        assert_eq!(m.distance, 0);
        assert_eq!(m.score, 1.0);
    }

    #[test]
    fn handles_transposition_and_substitution_typos() {
        let m = fuzzy_match("Tomet search engine", "srach", Some(2), false).unwrap();
        assert_eq!(m.distance, 2);

        let transp = fuzzy_match("Tomet search engine", "saerch", None, false).unwrap();
        assert_eq!(transp.distance, 1);
    }

    #[test]
    fn respects_max_distance_and_case_sensitivity() {
        assert!(fuzzy_match("alpha", "axxxa", Some(1), false).is_none());
        assert!(fuzzy_match("Tomet", "tomet", Some(0), true).is_none());
        assert!(fuzzy_match("Tomet", "tomet", Some(0), false).is_some());
    }
}
