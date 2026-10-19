//! Syntax shared by lexical retrieval and semantic candidate constraints.
//! Only unescaped uppercase AND/NOT and case-insensitive OR are operators.
//! Adjacency means AND; precedence is NOT, AND, OR. Invalid syntax fails closed.
use matric_core::{Error, Result};

#[derive(Clone)]
pub enum Expression {
    Term(String),
    Phrase(String),
    And(Box<Self>, Box<Self>),
    Or(Box<Self>, Box<Self>),
    Not(Box<Self>),
}

impl Expression {
    pub fn tsquery(&self) -> Result<String> {
        Ok(match self {
            Self::Term(term) => term.clone(),
            Self::Phrase(_) => {
                return Err(Error::InvalidInput(
                    "phrase must be analyzer-normalized before rendering tsquery".into(),
                ))
            }
            Self::And(a, b) => format!("({} & {})", a.tsquery()?, b.tsquery()?),
            Self::Or(a, b) => format!("({} | {})", a.tsquery()?, b.tsquery()?),
            Self::Not(a) => format!("!({})", a.tsquery()?),
        })
    }
}

#[derive(Clone)]
pub struct ParsedQuery {
    pub expression: Option<Expression>,
    pub explicit: bool,
}

#[derive(Clone, PartialEq)]
enum Token {
    Term(String),
    Phrase(String),
    And,
    Or,
    Not,
    Open,
    Close,
}

fn invalid() -> Error {
    Error::InvalidInput(
        "invalid search syntax: expected a term, balanced group, or quoted phrase".into(),
    )
}

/// Quote a literal for tsquery (not SQL). SQL always receives it as a bind value.
pub fn quote_term(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "''"))
}

impl ParsedQuery {
    pub fn parse(input: &str) -> Result<Self> {
        // Bound recursion and SQL expression size, including adversarial nesting.
        if input.len() > 8192 {
            return Err(invalid());
        }
        let mut chars = input.chars().peekable();
        let mut tokens = Vec::new();
        let mut explicit = false;
        while let Some(c) = chars.next() {
            if c.is_whitespace() {
                continue;
            }
            let token = match c {
                '(' => {
                    explicit = true;
                    Token::Open
                }
                ')' => {
                    explicit = true;
                    Token::Close
                }
                '-' => {
                    explicit = true;
                    Token::Not
                }
                '"' => {
                    explicit = true;
                    let mut phrase = String::new();
                    let mut closed = false;
                    while let Some(c) = chars.next() {
                        match c {
                            '"' => {
                                closed = true;
                                break;
                            }
                            '\\' => phrase.push(chars.next().ok_or_else(invalid)?),
                            _ => phrase.push(c),
                        }
                    }
                    if !closed || phrase.trim().is_empty() {
                        return Err(invalid());
                    }
                    Token::Phrase(phrase)
                }
                _ => {
                    let mut word = String::new();
                    let mut escaped = false;
                    let mut prefix = false;
                    let mut next = Some(c);
                    while let Some(c) = next {
                        if prefix {
                            return Err(invalid());
                        }
                        match c {
                            '\\' => {
                                escaped = true;
                                explicit = true;
                                word.push(chars.next().ok_or_else(invalid)?);
                            }
                            '*' => {
                                explicit = true;
                                prefix = true;
                            }
                            _ => word.push(c),
                        }
                        next = match chars.peek() {
                            Some(c) if !c.is_whitespace() && !matches!(c, '(' | ')' | '"') => {
                                chars.next()
                            }
                            _ => None,
                        };
                    }
                    if word.is_empty() {
                        return Err(invalid());
                    }
                    if !escaped && !prefix {
                        match word.as_str() {
                            "AND" => {
                                explicit = true;
                                Token::And
                            }
                            "NOT" => {
                                explicit = true;
                                Token::Not
                            }
                            _ if word.eq_ignore_ascii_case("OR") => {
                                explicit = true;
                                Token::Or
                            }
                            _ => Token::Term(quote_term(&word)),
                        }
                    } else {
                        Token::Term(format!(
                            "{}{}",
                            quote_term(&word),
                            if prefix { ":*" } else { "" }
                        ))
                    }
                }
            };
            tokens.push(token);
            if tokens.len() > 128 {
                return Err(invalid());
            }
        }
        if tokens.is_empty() {
            return Ok(Self {
                expression: None,
                explicit,
            });
        }
        let mut parser = Parser {
            tokens: &tokens,
            position: 0,
        };
        let expression = parser.or()?;
        if parser.position != tokens.len() {
            return Err(invalid());
        }
        Ok(Self {
            expression: Some(expression),
            explicit,
        })
    }

    pub fn tsquery(&self) -> Result<String> {
        self.expression
            .as_ref()
            .map(Expression::tsquery)
            .transpose()
            .map(Option::unwrap_or_default)
    }
}

struct Parser<'a> {
    tokens: &'a [Token],
    position: usize,
}
impl Parser<'_> {
    fn or(&mut self) -> Result<Expression> {
        let mut left = self.and()?;
        while self.tokens.get(self.position) == Some(&Token::Or) {
            self.position += 1;
            left = Expression::Or(Box::new(left), Box::new(self.and()?));
        }
        Ok(left)
    }
    fn and(&mut self) -> Result<Expression> {
        let mut left = self.unary()?;
        loop {
            match self.tokens.get(self.position) {
                Some(Token::And) => self.position += 1,
                Some(Token::Term(_) | Token::Phrase(_) | Token::Not | Token::Open) => {}
                _ => break,
            }
            left = Expression::And(Box::new(left), Box::new(self.unary()?));
        }
        Ok(left)
    }
    fn unary(&mut self) -> Result<Expression> {
        let token = self.tokens.get(self.position).ok_or_else(invalid)?;
        self.position += 1;
        match token {
            Token::Term(term) => Ok(Expression::Term(term.clone())),
            Token::Phrase(phrase) => Ok(Expression::Phrase(phrase.clone())),
            Token::Not => Ok(Expression::Not(Box::new(self.unary()?))),
            Token::Open => {
                let expression = self.or()?;
                if self.tokens.get(self.position) != Some(&Token::Close) {
                    return Err(invalid());
                }
                self.position += 1;
                Ok(expression)
            }
            _ => Err(invalid()),
        }
    }
}

impl Expression {
    fn leaves<'a>(&'a self, output: &mut Vec<(&'static str, &'a str)>) {
        match self {
            Self::Term(value) => output.push(("to_tsquery", value)),
            Self::Phrase(value) => output.push(("phraseto_tsquery", value)),
            Self::And(a, b) | Self::Or(a, b) => {
                a.leaves(output);
                b.leaves(output);
            }
            Self::Not(a) => a.leaves(output),
        }
    }

    /// Analyze whole phrases and tsquery terms in traversal order. Parameter $1
    /// is the analyzer configuration; user text is bound starting at $2.
    pub(crate) fn normalization_query(&self) -> (String, Vec<String>) {
        let mut leaves = Vec::new();
        self.leaves(&mut leaves);
        let queries = leaves
            .iter()
            .enumerate()
            .map(|(index, (function, _))| format!("{function}($1::regconfig, ${})", index + 2))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("SELECT query::text FROM unnest(ARRAY[{queries}]) WITH ORDINALITY AS t(query, position) ORDER BY position");
        (
            sql,
            leaves
                .into_iter()
                .map(|(_, value)| value.to_string())
                .collect(),
        )
    }

    pub(crate) fn normalized(&self, leaves: &mut impl Iterator<Item = String>) -> Option<Self> {
        match self {
            Self::Term(_) | Self::Phrase(_) => leaves
                .next()
                .filter(|value| !value.is_empty())
                .map(Self::Term),
            Self::Not(a) => a.normalized(leaves).map(|a| Self::Not(Box::new(a))),
            Self::And(a, b) | Self::Or(a, b) => {
                let a = a.normalized(leaves);
                let b = b.normalized(leaves);
                match (a, b) {
                    (Some(a), Some(b)) => Some(if matches!(self, Self::And(..)) {
                        Self::And(Box::new(a), Box::new(b))
                    } else {
                        Self::Or(Box::new(a), Box::new(b))
                    }),
                    (a, b) => a.or(b),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phrases_preserve_punctuation_and_escaped_quotes_as_bound_text() {
        for (input, expected) in [
            (r#""alpha , beta""#, "alpha , beta"),
            (r#""alpha the beta""#, "alpha the beta"),
            (r#""alpha \"beta\"""#, "alpha \"beta\""),
            (r#""alpha \'beta\' \\ gamma""#, "alpha 'beta' \\ gamma"),
            (
                r#""alpha '); SELECT 1; -- beta""#,
                "alpha '); SELECT 1; -- beta",
            ),
        ] {
            let parsed = ParsedQuery::parse(input).unwrap();
            assert!(parsed.explicit);
            let expression = parsed.expression.unwrap();
            assert!(matches!(&expression, Expression::Phrase(text) if text == expected));
            // Rendering a phrase without the database analyzer must fail closed.
            assert!(expression.tsquery().is_err());
            let (sql, params) = expression.normalization_query();
            assert_eq!(params, [expected]);
            assert!(sql.contains("phraseto_tsquery($1::regconfig, $2)"));
            assert!(!sql.contains(expected));
        }
    }

    #[test]
    fn mixed_expression_normalizes_terms_and_phrases_in_order() {
        let parsed =
            ParsedQuery::parse(r#"alpha AND ("beta , gamma" OR NOT "delta \"epsilon\"")"#).unwrap();
        let expression = parsed.expression.unwrap();
        let (sql, params) = expression.normalization_query();
        assert_eq!(params, ["'alpha'", "beta , gamma", "delta \"epsilon\""]);
        assert!(sql.contains("ARRAY[to_tsquery($1::regconfig, $2), phraseto_tsquery($1::regconfig, $3), phraseto_tsquery($1::regconfig, $4)]"));
        let normalized = expression
            .normalized(&mut ["'alpha'", "'beta'", ""].into_iter().map(String::from))
            .unwrap();
        assert_eq!(normalized.tsquery().unwrap(), "('alpha' & 'beta')");
    }

    #[test]
    fn parser_truth_table() {
        for (input, expected, explicit) in [
            ("alpha beta", "('alpha' & 'beta')", false),
            ("alpha AND beta", "('alpha' & 'beta')", true),
            (
                "alpha OR beta AND gamma",
                "('alpha' | ('beta' & 'gamma'))",
                true,
            ),
            (
                "(alpha OR beta) NOT gamma",
                "(('alpha' | 'beta') & !('gamma'))",
                true,
            ),
            ("alpha -beta", "('alpha' & !('beta'))", true),
            ("prefix*", "'prefix':*", true),
            ("alpha or beta", "('alpha' | 'beta')", true),
            (r"\OR", "'OR'", true),
            (r"alpha\*", "'alpha*'", true),
            ("full-text", "'full-text'", false),
            ("", "", false),
        ] {
            let parsed = ParsedQuery::parse(input).unwrap();
            assert_eq!(parsed.tsquery().unwrap(), expected, "{input}");
            assert_eq!(parsed.explicit, explicit, "{input}");
        }
    }
    #[test]
    fn malformed_input_never_relaxes() {
        for input in [
            "alpha OR",
            "AND alpha",
            "()",
            "(alpha",
            "alpha)",
            "\"alpha",
            "\"\"",
            "-",
            "*",
            "pre*fix",
            "alpha\\",
            "alpha AND OR beta",
        ] {
            assert!(ParsedQuery::parse(input).is_err(), "{input}");
        }
        assert!(ParsedQuery::parse(&"(".repeat(129)).is_err());
    }
}
