//! Boolean expression parser for combat log search.
//!
//! Supports AND, OR, NOT operators and parenthesized grouping.
//! Precedence: NOT (highest) > AND > OR (lowest).
//! Consecutive non-operator words form a single phrase term.

use super::sql_escape;

enum SearchExpr {
    Term(String),
    Not(Box<SearchExpr>),
    And(Box<SearchExpr>, Box<SearchExpr>),
    Or(Box<SearchExpr>, Box<SearchExpr>),
}

#[derive(PartialEq)]
enum Token {
    And,
    Or,
    Not,
    LParen,
    RParen,
    Term(String),
}

fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut term_parts: Vec<String> = Vec::new();
    let mut word = String::new();

    let flush_word = |word: &mut String, parts: &mut Vec<String>| {
        if !word.is_empty() {
            parts.push(std::mem::take(word));
        }
    };

    let flush_term = |parts: &mut Vec<String>, tokens: &mut Vec<Token>| {
        if !parts.is_empty() {
            tokens.push(Token::Term(parts.join(" ")));
            parts.clear();
        }
    };

    for ch in input.chars() {
        match ch {
            '(' | ')' => {
                flush_word(&mut word, &mut term_parts);
                flush_term(&mut term_parts, &mut tokens);
                tokens.push(if ch == '(' { Token::LParen } else { Token::RParen });
            }
            ' ' | '\t' => {
                if !word.is_empty() {
                    let upper = word.to_uppercase();
                    if matches!(upper.as_str(), "AND" | "OR" | "NOT") {
                        flush_term(&mut term_parts, &mut tokens);
                        tokens.push(match upper.as_str() {
                            "AND" => Token::And,
                            "OR" => Token::Or,
                            _ => Token::Not,
                        });
                        word.clear();
                    } else {
                        flush_word(&mut word, &mut term_parts);
                    }
                }
            }
            _ => word.push(ch),
        }
    }
    flush_word(&mut word, &mut term_parts);
    flush_term(&mut term_parts, &mut tokens);
    tokens
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) {
        self.pos += 1;
    }

    fn parse(mut self) -> Option<SearchExpr> {
        let expr = self.parse_or();
        if expr.is_some() && self.pos < self.tokens.len() {
            return None;
        }
        expr
    }

    fn parse_or(&mut self) -> Option<SearchExpr> {
        let mut left = self.parse_and()?;
        while self.peek() == Some(&Token::Or) {
            self.advance();
            let right = self.parse_and()?;
            left = SearchExpr::Or(Box::new(left), Box::new(right));
        }
        Some(left)
    }

    fn parse_and(&mut self) -> Option<SearchExpr> {
        let mut left = self.parse_not()?;
        while self.peek() == Some(&Token::And) {
            self.advance();
            let right = self.parse_not()?;
            left = SearchExpr::And(Box::new(left), Box::new(right));
        }
        Some(left)
    }

    fn parse_not(&mut self) -> Option<SearchExpr> {
        if self.peek() == Some(&Token::Not) {
            self.advance();
            let expr = self.parse_not()?;
            return Some(SearchExpr::Not(Box::new(expr)));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Option<SearchExpr> {
        match self.peek()? {
            Token::LParen => {
                self.advance();
                let expr = self.parse_or()?;
                if self.peek() == Some(&Token::RParen) {
                    self.advance();
                }
                Some(expr)
            }
            Token::Term(_) => {
                let Token::Term(s) = std::mem::replace(
                    &mut self.tokens[self.pos],
                    Token::Term(String::new()),
                ) else {
                    unreachable!()
                };
                self.pos += 1;
                Some(SearchExpr::Term(s))
            }
            _ => None,
        }
    }
}

fn term_to_sql(term: &str) -> String {
    // `id:123` / `src_id:123` / `tgt_id:123` pin an exact instance id instead of a
    // substring match, so a short id can't match inside a longer one.
    let id_term = |prefix: &str| term.strip_prefix(prefix).and_then(|v| v.parse::<i64>().ok());
    if let Some(id) = id_term("src_id:") {
        return format!("source_id = {id}");
    }
    if let Some(id) = id_term("tgt_id:") {
        return format!("target_id = {id}");
    }
    if let Some(id) = id_term("id:") {
        return format!("(source_id = {id} OR target_id = {id})");
    }
    let escaped = sql_escape(term).to_lowercase();
    format!(
        "(LOWER(source_name) LIKE '%{0}%' OR LOWER(target_name) LIKE '%{0}%' OR LOWER(ability_name) LIKE '%{0}%' OR LOWER(effect_name) LIKE '%{0}%' OR CAST(ability_id AS VARCHAR) LIKE '%{0}%' OR CAST(effect_id AS VARCHAR) LIKE '%{0}%' OR CAST(source_id AS VARCHAR) LIKE '%{0}%' OR CAST(target_id AS VARCHAR) LIKE '%{0}%' OR CAST(source_class_id AS VARCHAR) LIKE '%{0}%' OR CAST(target_class_id AS VARCHAR) LIKE '%{0}%')",
        escaped
    )
}

fn expr_to_sql(expr: &SearchExpr) -> String {
    match expr {
        SearchExpr::Term(t) => term_to_sql(t),
        SearchExpr::Not(inner) => format!("(NOT {})", expr_to_sql(inner)),
        SearchExpr::And(l, r) => format!("({} AND {})", expr_to_sql(l), expr_to_sql(r)),
        SearchExpr::Or(l, r) => format!("({} OR {})", expr_to_sql(l), expr_to_sql(r)),
    }
}

pub(super) fn build_search_clause(search: &str) -> String {
    let search = search.trim();
    if search.is_empty() {
        return "1=1".to_string();
    }
    let tokens = tokenize(search);
    if tokens.is_empty() {
        return "1=1".to_string();
    }
    let parser = Parser { tokens, pos: 0 };
    match parser.parse() {
        Some(expr) => expr_to_sql(&expr),
        None => term_to_sql(search),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> String {
        term_to_sql(s)
    }

    #[test]
    fn empty_input() {
        assert_eq!(build_search_clause(""), "1=1");
        assert_eq!(build_search_clause("   "), "1=1");
    }

    #[test]
    fn instance_id_terms() {
        assert_eq!(build_search_clause("id:42"), "(source_id = 42 OR target_id = 42)");
        assert_eq!(build_search_clause("src_id:42"), "source_id = 42");
        assert_eq!(build_search_clause("tgt_id:42 AND NOT src_id:7"),
            "(target_id = 42 AND (NOT source_id = 7))");
        // Non-numeric suffix falls back to a substring term
        assert_eq!(build_search_clause("id:abc"), t("id:abc"));
    }

    #[test]
    fn single_term() {
        assert_eq!(build_search_clause("knight"), t("knight"));
    }

    #[test]
    fn multi_word_phrase() {
        assert_eq!(build_search_clause("force lightning"), t("force lightning"));
    }

    #[test]
    fn simple_or() {
        let result = build_search_clause("knight OR warrior");
        assert_eq!(result, format!("({} OR {})", t("knight"), t("warrior")));
    }

    #[test]
    fn simple_and() {
        let result = build_search_clause("knight AND damage");
        assert_eq!(result, format!("({} AND {})", t("knight"), t("damage")));
    }

    #[test]
    fn simple_not() {
        let result = build_search_clause("NOT knight");
        assert_eq!(result, format!("(NOT {})", t("knight")));
    }

    #[test]
    fn and_or_precedence() {
        // A AND B OR C  →  (A AND B) OR C
        let result = build_search_clause("knight AND damage OR heal");
        let expected = format!("(({} AND {}) OR {})", t("knight"), t("damage"), t("heal"));
        assert_eq!(result, expected);
    }

    #[test]
    fn or_and_precedence() {
        // A OR B AND C  →  A OR (B AND C)
        let result = build_search_clause("heal OR knight AND damage");
        let expected = format!("({} OR ({} AND {}))", t("heal"), t("knight"), t("damage"));
        assert_eq!(result, expected);
    }

    #[test]
    fn parens_override_precedence() {
        // (A OR B) AND C
        let result = build_search_clause("(knight OR warrior) AND damage");
        let expected = format!(
            "(({} OR {}) AND {})",
            t("knight"),
            t("warrior"),
            t("damage")
        );
        assert_eq!(result, expected);
    }

    #[test]
    fn nested_parens() {
        let result = build_search_clause("(A AND B) OR (C AND D)");
        let expected = format!(
            "(({} AND {}) OR ({} AND {}))",
            t("a"),
            t("b"),
            t("c"),
            t("d"),
        );
        assert_eq!(result, expected);
    }

    #[test]
    fn not_with_parens() {
        let result = build_search_clause("NOT (knight OR warrior)");
        let expected = format!("(NOT ({} OR {}))", t("knight"), t("warrior"));
        assert_eq!(result, expected);
    }

    #[test]
    fn complex_expression() {
        // (A AND B) OR NOT C
        let result = build_search_clause("(knight AND damage) OR NOT heal");
        let expected = format!(
            "(({} AND {}) OR (NOT {}))",
            t("knight"),
            t("damage"),
            t("heal")
        );
        assert_eq!(result, expected);
    }

    #[test]
    fn case_insensitive_operators() {
        let upper = build_search_clause("A AND B");
        let lower = build_search_clause("A and B");
        let mixed = build_search_clause("A And B");
        assert_eq!(upper, lower);
        assert_eq!(upper, mixed);
    }

    #[test]
    fn malformed_falls_back_to_term() {
        // Dangling operator — entire input treated as a single term
        let result = build_search_clause("AND knight");
        assert_eq!(result, t("and knight"));
    }

    #[test]
    fn backward_compat_or_not() {
        // Old-style "A OR B OR NOT C" still works
        let result = build_search_clause("knight OR warrior OR NOT boss");
        let expected = format!(
            "(({} OR {}) OR (NOT {}))",
            t("knight"),
            t("warrior"),
            t("boss")
        );
        assert_eq!(result, expected);
    }

    #[test]
    fn sql_injection_escaped() {
        let result = build_search_clause("O'Brien");
        assert!(result.contains("o''brien"));
    }
}
