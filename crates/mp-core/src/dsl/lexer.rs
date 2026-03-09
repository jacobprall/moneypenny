use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Keywords (case-insensitive, stored uppercase)
    Keyword(Kw),
    // Identifiers (field names, store names, etc.)
    Ident(String),
    // Literals
    StringLit(String),
    IntLit(i64),
    FloatLit(f64),
    BoolLit(bool),
    DurationLit(u64, char), // (amount, unit char: 'd','h','m','s')
    // Operators
    Eq,       // =
    Ne,       // !=
    Gt,       // >
    Lt,       // <
    Ge,       // >=
    Le,       // <=
    // Punctuation
    Pipe,      // |
    Semicolon, // ;
    Comma,     // ,
    LParen,    // (
    RParen,    // )
    // JSON blob (grabbed as raw string)
    JsonBlob(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kw {
    Search, Insert, Update, Delete, From, Into, Set, Where,
    And, Or, Not, Like,
    Since, Before, Scope, Agent, Mode,
    Sort, Asc, Desc, Take, Offset, Count,
    Ingest, As,
    Create, Evaluate, Explain, Policy, Allow, Deny, Audit, On, For, Message,
    Job, Schedule, Type, Payload, Run, Pause, Resume, List, History,
    Config, Skill, Promote, Tool, Language, Body,
    Embedding, Status, Retry, Dead, Backfill, Process,
    Session, Resolve,
    Exec, Events,
}

impl Kw {
    fn from_str(s: &str) -> Option<Kw> {
        match s.to_ascii_uppercase().as_str() {
            "SEARCH" => Some(Kw::Search),
            "INSERT" => Some(Kw::Insert),
            "UPDATE" => Some(Kw::Update),
            "DELETE" => Some(Kw::Delete),
            "FROM" => Some(Kw::From),
            "INTO" => Some(Kw::Into),
            "SET" => Some(Kw::Set),
            "WHERE" => Some(Kw::Where),
            "AND" => Some(Kw::And),
            "OR" => Some(Kw::Or),
            "NOT" => Some(Kw::Not),
            "LIKE" => Some(Kw::Like),
            "SINCE" => Some(Kw::Since),
            "BEFORE" => Some(Kw::Before),
            "SCOPE" => Some(Kw::Scope),
            "AGENT" => Some(Kw::Agent),
            "MODE" => Some(Kw::Mode),
            "SORT" => Some(Kw::Sort),
            "ASC" => Some(Kw::Asc),
            "DESC" => Some(Kw::Desc),
            "TAKE" => Some(Kw::Take),
            "OFFSET" => Some(Kw::Offset),
            "COUNT" => Some(Kw::Count),
            "INGEST" => Some(Kw::Ingest),
            "AS" => Some(Kw::As),
            "CREATE" => Some(Kw::Create),
            "EVALUATE" => Some(Kw::Evaluate),
            "EXPLAIN" => Some(Kw::Explain),
            "POLICY" => Some(Kw::Policy),
            "ALLOW" => Some(Kw::Allow),
            "DENY" => Some(Kw::Deny),
            "AUDIT" => Some(Kw::Audit),
            "ON" => Some(Kw::On),
            "FOR" => Some(Kw::For),
            "MESSAGE" => Some(Kw::Message),
            "JOB" => Some(Kw::Job),
            "SCHEDULE" => Some(Kw::Schedule),
            "TYPE" => Some(Kw::Type),
            "PAYLOAD" => Some(Kw::Payload),
            "RUN" => Some(Kw::Run),
            "PAUSE" => Some(Kw::Pause),
            "RESUME" => Some(Kw::Resume),
            "LIST" => Some(Kw::List),
            "HISTORY" => Some(Kw::History),
            "CONFIG" => Some(Kw::Config),
            "SKILL" => Some(Kw::Skill),
            "PROMOTE" => Some(Kw::Promote),
            "TOOL" => Some(Kw::Tool),
            "LANGUAGE" => Some(Kw::Language),
            "BODY" => Some(Kw::Body),
            "EMBEDDING" => Some(Kw::Embedding),
            "STATUS" => Some(Kw::Status),
            "RETRY" => Some(Kw::Retry),
            "DEAD" => Some(Kw::Dead),
            "BACKFILL" => Some(Kw::Backfill),
            "PROCESS" => Some(Kw::Process),
            "SESSION" => Some(Kw::Session),
            "RESOLVE" => Some(Kw::Resolve),
            "EXEC" => Some(Kw::Exec),
            "EVENTS" => Some(Kw::Events),
            _ => None,
        }
    }
}

impl fmt::Display for Kw {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Kw::Search => "SEARCH", Kw::Insert => "INSERT", Kw::Update => "UPDATE",
            Kw::Delete => "DELETE", Kw::From => "FROM", Kw::Into => "INTO",
            Kw::Set => "SET", Kw::Where => "WHERE", Kw::And => "AND",
            Kw::Or => "OR", Kw::Not => "NOT", Kw::Like => "LIKE",
            Kw::Since => "SINCE", Kw::Before => "BEFORE", Kw::Scope => "SCOPE",
            Kw::Agent => "AGENT", Kw::Mode => "MODE", Kw::Sort => "SORT",
            Kw::Asc => "ASC", Kw::Desc => "DESC", Kw::Take => "TAKE",
            Kw::Offset => "OFFSET", Kw::Count => "COUNT", Kw::Ingest => "INGEST",
            Kw::As => "AS", Kw::Create => "CREATE", Kw::Evaluate => "EVALUATE",
            Kw::Explain => "EXPLAIN", Kw::Policy => "POLICY", Kw::Allow => "ALLOW",
            Kw::Deny => "DENY", Kw::Audit => "AUDIT", Kw::On => "ON",
            Kw::For => "FOR", Kw::Message => "MESSAGE", Kw::Job => "JOB",
            Kw::Schedule => "SCHEDULE", Kw::Type => "TYPE", Kw::Payload => "PAYLOAD",
            Kw::Run => "RUN", Kw::Pause => "PAUSE", Kw::Resume => "RESUME",
            Kw::List => "LIST", Kw::History => "HISTORY", Kw::Config => "CONFIG",
            Kw::Skill => "SKILL", Kw::Promote => "PROMOTE", Kw::Tool => "TOOL",
            Kw::Language => "LANGUAGE", Kw::Body => "BODY", Kw::Embedding => "EMBEDDING",
            Kw::Status => "STATUS", Kw::Retry => "RETRY", Kw::Dead => "DEAD",
            Kw::Backfill => "BACKFILL", Kw::Process => "PROCESS",
            Kw::Session => "SESSION", Kw::Resolve => "RESOLVE",
            Kw::Exec => "EXEC", Kw::Events => "EVENTS",
        };
        write!(f, "{s}")
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Keyword(kw) => write!(f, "{kw}"),
            Token::Ident(s) => write!(f, "{s}"),
            Token::StringLit(s) => write!(f, "\"{s}\""),
            Token::IntLit(n) => write!(f, "{n}"),
            Token::FloatLit(n) => write!(f, "{n}"),
            Token::BoolLit(b) => write!(f, "{b}"),
            Token::DurationLit(n, u) => write!(f, "{n}{u}"),
            Token::Eq => write!(f, "="),
            Token::Ne => write!(f, "!="),
            Token::Gt => write!(f, ">"),
            Token::Lt => write!(f, "<"),
            Token::Ge => write!(f, ">="),
            Token::Le => write!(f, "<="),
            Token::Pipe => write!(f, "|"),
            Token::Semicolon => write!(f, ";"),
            Token::Comma => write!(f, ","),
            Token::LParen => write!(f, "("),
            Token::RParen => write!(f, ")"),
            Token::JsonBlob(s) => write!(f, "{s}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Spanned {
    pub token: Token,
    pub pos: usize,
}

#[derive(Debug, Clone)]
pub struct LexError {
    pub pos: usize,
    pub message: String,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "lex error at position {}: {}", self.pos, self.message)
    }
}

pub fn lex(input: &str) -> Result<Vec<Spanned>, LexError> {
    let mut tokens = Vec::new();
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Skip whitespace
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }

        let start = i;

        // String literal
        if bytes[i] == b'"' {
            i += 1;
            let mut s = String::new();
            let mut escaped = false;
            loop {
                if i >= len {
                    return Err(LexError {
                        pos: start,
                        message: "unterminated string literal".into(),
                    });
                }
                if escaped {
                    s.push(bytes[i] as char);
                    escaped = false;
                    i += 1;
                    continue;
                }
                if bytes[i] == b'\\' {
                    escaped = true;
                    i += 1;
                    continue;
                }
                if bytes[i] == b'"' {
                    i += 1;
                    break;
                }
                s.push(bytes[i] as char);
                i += 1;
            }
            tokens.push(Spanned {
                token: Token::StringLit(s),
                pos: start,
            });
            continue;
        }

        // JSON blob: starts with {, grab until matching }
        if bytes[i] == b'{' {
            let mut depth = 0i32;
            let mut j = i;
            let mut in_str = false;
            let mut esc = false;
            loop {
                if j >= len {
                    return Err(LexError {
                        pos: start,
                        message: "unterminated JSON blob".into(),
                    });
                }
                let ch = bytes[j];
                if esc {
                    esc = false;
                    j += 1;
                    continue;
                }
                if ch == b'\\' && in_str {
                    esc = true;
                    j += 1;
                    continue;
                }
                if ch == b'"' {
                    in_str = !in_str;
                }
                if !in_str {
                    if ch == b'{' {
                        depth += 1;
                    } else if ch == b'}' {
                        depth -= 1;
                        if depth == 0 {
                            j += 1;
                            break;
                        }
                    }
                }
                j += 1;
            }
            let blob = &input[i..j];
            tokens.push(Spanned {
                token: Token::JsonBlob(blob.to_string()),
                pos: start,
            });
            i = j;
            continue;
        }

        // Two-char operators
        if i + 1 < len {
            let two = &input[i..i + 2];
            let tok = match two {
                "!=" => Some(Token::Ne),
                ">=" => Some(Token::Ge),
                "<=" => Some(Token::Le),
                _ => None,
            };
            if let Some(t) = tok {
                tokens.push(Spanned { token: t, pos: start });
                i += 2;
                continue;
            }
        }

        // Single-char operators/punctuation
        let single = match bytes[i] {
            b'=' => Some(Token::Eq),
            b'>' => Some(Token::Gt),
            b'<' => Some(Token::Lt),
            b'|' => Some(Token::Pipe),
            b';' => Some(Token::Semicolon),
            b',' => Some(Token::Comma),
            b'(' => Some(Token::LParen),
            b')' => Some(Token::RParen),
            _ => None,
        };
        if let Some(t) = single {
            tokens.push(Spanned { token: t, pos: start });
            i += 1;
            continue;
        }

        // Number or duration: starts with digit or negative sign followed by digit
        if bytes[i].is_ascii_digit()
            || (bytes[i] == b'-' && i + 1 < len && bytes[i + 1].is_ascii_digit())
        {
            let negative = bytes[i] == b'-';
            if negative {
                i += 1;
            }
            let num_start = i;
            let mut has_dot = false;
            while i < len && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                if bytes[i] == b'.' {
                    if has_dot {
                        break;
                    }
                    has_dot = true;
                }
                i += 1;
            }
            // Check for duration suffix
            if i < len && matches!(bytes[i], b'd' | b'h' | b'm' | b's') && !has_dot {
                let num_str = &input[num_start..i];
                let amount: u64 = num_str.parse().map_err(|_| LexError {
                    pos: start,
                    message: format!("invalid duration number: {num_str}"),
                })?;
                let unit = bytes[i] as char;
                i += 1;
                tokens.push(Spanned {
                    token: Token::DurationLit(amount, unit),
                    pos: start,
                });
                continue;
            }
            let num_str = &input[num_start..i];
            if has_dot {
                let n: f64 = num_str.parse().map_err(|_| LexError {
                    pos: start,
                    message: format!("invalid float: {num_str}"),
                })?;
                let n = if negative { -n } else { n };
                tokens.push(Spanned {
                    token: Token::FloatLit(n),
                    pos: start,
                });
            } else {
                let n: i64 = num_str.parse().map_err(|_| LexError {
                    pos: start,
                    message: format!("invalid integer: {num_str}"),
                })?;
                let n = if negative { -n } else { n };
                tokens.push(Spanned {
                    token: Token::IntLit(n),
                    pos: start,
                });
            }
            continue;
        }

        // Identifier or keyword: starts with alpha or underscore
        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            let word_start = i;
            while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'-') {
                i += 1;
            }
            let word = &input[word_start..i];

            // Check booleans
            if word.eq_ignore_ascii_case("true") {
                tokens.push(Spanned {
                    token: Token::BoolLit(true),
                    pos: start,
                });
                continue;
            }
            if word.eq_ignore_ascii_case("false") {
                tokens.push(Spanned {
                    token: Token::BoolLit(false),
                    pos: start,
                });
                continue;
            }

            // Check keywords
            if let Some(kw) = Kw::from_str(word) {
                tokens.push(Spanned {
                    token: Token::Keyword(kw),
                    pos: start,
                });
            } else {
                tokens.push(Spanned {
                    token: Token::Ident(word.to_string()),
                    pos: start,
                });
            }
            continue;
        }

        // Unknown character
        return Err(LexError {
            pos: i,
            message: format!("unexpected character: '{}'", bytes[i] as char),
        });
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lex_simple_search() {
        let tokens = lex(r#"SEARCH facts WHERE topic = "auth" SINCE 7d | TAKE 10"#).unwrap();
        assert!(matches!(tokens[0].token, Token::Keyword(Kw::Search)));
        assert!(matches!(tokens[1].token, Token::Ident(ref s) if s == "facts"));
        assert!(matches!(tokens[2].token, Token::Keyword(Kw::Where)));
        assert!(matches!(tokens[3].token, Token::Ident(ref s) if s == "topic"));
        assert!(matches!(tokens[4].token, Token::Eq));
        assert!(matches!(tokens[5].token, Token::StringLit(ref s) if s == "auth"));
        assert!(matches!(tokens[6].token, Token::Keyword(Kw::Since)));
        assert!(matches!(tokens[7].token, Token::DurationLit(7, 'd')));
        assert!(matches!(tokens[8].token, Token::Pipe));
        assert!(matches!(tokens[9].token, Token::Keyword(Kw::Take)));
        assert!(matches!(tokens[10].token, Token::IntLit(10)));
    }

    #[test]
    fn lex_insert() {
        let tokens = lex(r#"INSERT INTO facts ("Redis is preferred", topic="infra", confidence=0.9)"#).unwrap();
        assert!(matches!(tokens[0].token, Token::Keyword(Kw::Insert)));
        assert!(matches!(tokens[1].token, Token::Keyword(Kw::Into)));
        // INSERT INTO facts ( "Redis is preferred" , topic = "infra" , confidence = 0.9 )
        assert!(matches!(tokens[4].token, Token::StringLit(ref s) if s == "Redis is preferred"));
        assert!(matches!(tokens[12].token, Token::FloatLit(n) if (n - 0.9).abs() < f64::EPSILON));
    }

    #[test]
    fn lex_json_blob() {
        let tokens = lex(r#"CREATE JOB "test" SCHEDULE "* * *" PAYLOAD {"key": "val"}"#).unwrap();
        let last = &tokens[tokens.len() - 1];
        assert!(matches!(last.token, Token::JsonBlob(ref s) if s.contains("key")));
    }

    #[test]
    fn lex_multi_statement() {
        let tokens = lex(r#"DELETE FROM facts WHERE id = "x"; SEARCH facts | COUNT"#).unwrap();
        let semis: Vec<_> = tokens.iter().filter(|t| matches!(t.token, Token::Semicolon)).collect();
        assert_eq!(semis.len(), 1);
    }

    #[test]
    fn lex_comparison_operators() {
        let tokens = lex(r#"confidence >= 0.5 AND age != 3 AND score <= 100"#).unwrap();
        assert!(tokens.iter().any(|t| matches!(t.token, Token::Ge)));
        assert!(tokens.iter().any(|t| matches!(t.token, Token::Ne)));
        assert!(tokens.iter().any(|t| matches!(t.token, Token::Le)));
    }

    #[test]
    fn lex_error_unterminated_string() {
        let result = lex(r#"SEARCH facts WHERE topic = "auth"#);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("unterminated"));
    }

    #[test]
    fn lex_exec_keyword() {
        let tokens = lex(r#"EXEC "op.name" {"key": "val"}"#).unwrap();
        assert!(matches!(tokens[0].token, Token::Keyword(Kw::Exec)));
        assert!(matches!(tokens[1].token, Token::StringLit(ref s) if s == "op.name"));
        assert!(matches!(tokens[2].token, Token::JsonBlob(_)));
    }

    #[test]
    fn lex_events_keyword() {
        let tokens = lex(r#"INGEST EVENTS "cursor""#).unwrap();
        assert!(matches!(tokens[0].token, Token::Keyword(Kw::Ingest)));
        assert!(matches!(tokens[1].token, Token::Keyword(Kw::Events)));
        assert!(matches!(tokens[2].token, Token::StringLit(ref s) if s == "cursor"));
    }
}
