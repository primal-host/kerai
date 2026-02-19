/// Structural kind for a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Word,
    LParen,
    RParen,
    LBracket,
    RBracket,
}

/// A single token from a kerai line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub value: String,
    pub quoted: bool,
    pub kind: TokenKind,
}

/// Tokenize a line by splitting on whitespace, respecting quoted strings.
///
/// - Single and double quotes are supported
/// - Backslash escapes inside quotes: `\"`, `\'`, `\\`
/// - Quotes are stripped from the value; `quoted` is set to `true`
/// - Dot-namespaced identifiers (e.g. `postgres.global.connection`) are naturally one token
/// - `(` and `)` are structural delimiters (LParen/RParen), unless quoted
pub fn tokenize(line: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = line.chars().peekable();

    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }

        // Parentheses as structural delimiters
        if ch == '(' {
            chars.next();
            tokens.push(Token {
                value: "(".to_string(),
                quoted: false,
                kind: TokenKind::LParen,
            });
            continue;
        }
        if ch == ')' {
            chars.next();
            tokens.push(Token {
                value: ")".to_string(),
                quoted: false,
                kind: TokenKind::RParen,
            });
            continue;
        }

        if ch == '[' {
            chars.next();
            tokens.push(Token {
                value: "[".to_string(),
                quoted: false,
                kind: TokenKind::LBracket,
            });
            continue;
        }
        if ch == ']' {
            chars.next();
            tokens.push(Token {
                value: "]".to_string(),
                quoted: false,
                kind: TokenKind::RBracket,
            });
            continue;
        }

        if ch == '\'' || ch == '"' {
            let quote = ch;
            chars.next(); // consume opening quote
            let mut value = String::new();
            loop {
                match chars.next() {
                    Some('\\') => {
                        // Escaped character inside quotes
                        if let Some(escaped) = chars.next() {
                            match escaped {
                                '\\' | '\'' | '"' => value.push(escaped),
                                'n' => value.push('\n'),
                                't' => value.push('\t'),
                                other => {
                                    value.push('\\');
                                    value.push(other);
                                }
                            }
                        }
                    }
                    Some(c) if c == quote => break,
                    Some(c) => value.push(c),
                    None => break, // unterminated quote — take what we have
                }
            }
            tokens.push(Token {
                value,
                quoted: true,
                kind: TokenKind::Word,
            });
        } else {
            let mut value = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() || c == '(' || c == ')' || c == '[' || c == ']' {
                    break;
                }
                value.push(c);
                chars.next();
            }
            tokens.push(Token {
                value,
                quoted: false,
                kind: TokenKind::Word,
            });
        }
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_words() {
        let tokens = tokenize("hello world");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].value, "hello");
        assert!(!tokens[0].quoted);
        assert_eq!(tokens[0].kind, TokenKind::Word);
        assert_eq!(tokens[1].value, "world");
    }

    #[test]
    fn double_quoted_string() {
        let tokens = tokenize(r#"key "hello world""#);
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].value, "key");
        assert!(!tokens[0].quoted);
        assert_eq!(tokens[1].value, "hello world");
        assert!(tokens[1].quoted);
        assert_eq!(tokens[1].kind, TokenKind::Word);
    }

    #[test]
    fn single_quoted_string() {
        let tokens = tokenize("key 'hello world'");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[1].value, "hello world");
        assert!(tokens[1].quoted);
    }

    #[test]
    fn escaped_quote_inside() {
        let tokens = tokenize(r#""say \"hi\"""#);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].value, r#"say "hi""#);
    }

    #[test]
    fn escaped_backslash() {
        let tokens = tokenize(r#""path\\to""#);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].value, r"path\to");
    }

    #[test]
    fn dot_namespaced() {
        let tokens = tokenize("postgres.global.connection localhost");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].value, "postgres.global.connection");
        assert!(!tokens[0].quoted);
    }

    #[test]
    fn empty_line() {
        let tokens = tokenize("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn whitespace_only() {
        let tokens = tokenize("   \t  ");
        assert!(tokens.is_empty());
    }

    #[test]
    fn mixed_quotes_and_plain() {
        let tokens = tokenize(r#"cmd "arg one" plain 'arg two'"#);
        assert_eq!(tokens.len(), 4);
        assert_eq!(tokens[0].value, "cmd");
        assert!(!tokens[0].quoted);
        assert_eq!(tokens[1].value, "arg one");
        assert!(tokens[1].quoted);
        assert_eq!(tokens[2].value, "plain");
        assert!(!tokens[2].quoted);
        assert_eq!(tokens[3].value, "arg two");
        assert!(tokens[3].quoted);
    }

    #[test]
    fn unterminated_quote() {
        let tokens = tokenize(r#""unterminated"#);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].value, "unterminated");
        assert!(tokens[0].quoted);
    }

    #[test]
    fn paren_delimiters() {
        let tokens = tokenize("(1 + 2) * 3");
        assert_eq!(tokens.len(), 7);
        assert_eq!(tokens[0].kind, TokenKind::LParen);
        assert_eq!(tokens[0].value, "(");
        assert_eq!(tokens[1].value, "1");
        assert_eq!(tokens[1].kind, TokenKind::Word);
        assert_eq!(tokens[2].value, "+");
        assert_eq!(tokens[3].value, "2");
        assert_eq!(tokens[4].kind, TokenKind::RParen);
        assert_eq!(tokens[4].value, ")");
        assert_eq!(tokens[5].value, "*");
        assert_eq!(tokens[6].value, "3");
    }

    #[test]
    fn adjacent_paren_breaks_word() {
        let tokens = tokenize("foo(bar)");
        assert_eq!(tokens.len(), 4);
        assert_eq!(tokens[0].value, "foo");
        assert_eq!(tokens[0].kind, TokenKind::Word);
        assert_eq!(tokens[1].kind, TokenKind::LParen);
        assert_eq!(tokens[2].value, "bar");
        assert_eq!(tokens[2].kind, TokenKind::Word);
        assert_eq!(tokens[3].kind, TokenKind::RParen);
    }

    #[test]
    fn bracket_delimiters() {
        let tokens = tokenize("[1 2 3]");
        assert_eq!(tokens.len(), 5);
        assert_eq!(tokens[0].kind, TokenKind::LBracket);
        assert_eq!(tokens[0].value, "[");
        assert_eq!(tokens[1].value, "1");
        assert_eq!(tokens[1].kind, TokenKind::Word);
        assert_eq!(tokens[2].value, "2");
        assert_eq!(tokens[3].value, "3");
        assert_eq!(tokens[4].kind, TokenKind::RBracket);
        assert_eq!(tokens[4].value, "]");
    }

    #[test]
    fn adjacent_bracket_breaks_word() {
        let tokens = tokenize("foo[bar]");
        assert_eq!(tokens.len(), 4);
        assert_eq!(tokens[0].value, "foo");
        assert_eq!(tokens[0].kind, TokenKind::Word);
        assert_eq!(tokens[1].kind, TokenKind::LBracket);
        assert_eq!(tokens[2].value, "bar");
        assert_eq!(tokens[2].kind, TokenKind::Word);
        assert_eq!(tokens[3].kind, TokenKind::RBracket);
    }

    #[test]
    fn quoted_bracket_not_special() {
        let tokens = tokenize(r#""[" hello "]""#);
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].value, "[");
        assert!(tokens[0].quoted);
        assert_eq!(tokens[0].kind, TokenKind::Word);
        assert_eq!(tokens[1].value, "hello");
        assert_eq!(tokens[2].value, "]");
        assert!(tokens[2].quoted);
        assert_eq!(tokens[2].kind, TokenKind::Word);
    }

    #[test]
    fn nested_brackets() {
        let tokens = tokenize("[[1] 2]");
        assert_eq!(tokens.len(), 6);
        assert_eq!(tokens[0].kind, TokenKind::LBracket);
        assert_eq!(tokens[1].kind, TokenKind::LBracket);
        assert_eq!(tokens[2].value, "1");
        assert_eq!(tokens[3].kind, TokenKind::RBracket);
        assert_eq!(tokens[4].value, "2");
        assert_eq!(tokens[5].kind, TokenKind::RBracket);
    }

    #[test]
    fn quoted_paren_not_special() {
        let tokens = tokenize(r#""(" hello ")""#);
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].value, "(");
        assert!(tokens[0].quoted);
        assert_eq!(tokens[0].kind, TokenKind::Word);
        assert_eq!(tokens[1].value, "hello");
        assert_eq!(tokens[2].value, ")");
        assert!(tokens[2].quoted);
        assert_eq!(tokens[2].kind, TokenKind::Word);
    }
}
