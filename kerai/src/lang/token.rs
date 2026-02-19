/// A single token from a kerai line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub value: String,
    pub quoted: bool,
}

/// Tokenize a line by splitting on whitespace, respecting quoted strings.
///
/// - Single and double quotes are supported
/// - Backslash escapes inside quotes: `\"`, `\'`, `\\`
/// - Quotes are stripped from the value; `quoted` is set to `true`
/// - Dot-namespaced identifiers (e.g. `postgres.global.connection`) are naturally one token
pub fn tokenize(line: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = line.chars().peekable();

    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
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
            });
        } else {
            let mut value = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                value.push(c);
                chars.next();
            }
            tokens.push(Token {
                value,
                quoted: false,
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
}
