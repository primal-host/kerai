use super::ast::{Document, Line, Notation};
use super::token::tokenize;

/// Parser with a notation mode stack for `.kerai` files.
pub struct Parser {
    notation_stack: Vec<Notation>,
}

impl Parser {
    pub fn new() -> Self {
        Parser {
            notation_stack: vec![Notation::Prefix],
        }
    }

    /// Current notation mode (top of stack).
    fn notation(&self) -> Notation {
        *self.notation_stack.last().unwrap_or(&Notation::Prefix)
    }

    /// Replace the current notation mode (mutate top of stack).
    fn set_notation(&mut self, notation: Notation) {
        if let Some(top) = self.notation_stack.last_mut() {
            *top = notation;
        }
    }

    /// Push a notation mode onto the stack (for entering a function body — future).
    #[allow(dead_code)]
    pub fn push_notation(&mut self, notation: Notation) {
        self.notation_stack.push(notation);
    }

    /// Pop a notation mode from the stack (for exiting a function body — future).
    /// Never pops below depth 1.
    #[allow(dead_code)]
    pub fn pop_notation(&mut self) {
        if self.notation_stack.len() > 1 {
            self.notation_stack.pop();
        }
    }

    /// Parse a complete document from source text.
    pub fn parse(&mut self, source: &str) -> Document {
        let mut doc = Document::new();

        for line in source.lines() {
            let parsed = self.parse_line(line);
            doc.lines.push(parsed);
        }

        doc.default_notation = self.notation();
        doc
    }

    /// Parse a single line into a `Line` variant.
    fn parse_line(&mut self, raw: &str) -> Line {
        let trimmed = raw.trim();

        // Empty line
        if trimmed.is_empty() {
            return Line::Empty;
        }

        // Comment line
        if trimmed.starts_with('#') || trimmed.starts_with("//") {
            return Line::Comment {
                text: raw.to_string(),
            };
        }

        // Definition line: `:name target`
        if let Some(rest) = trimmed.strip_prefix(':') {
            let tokens = tokenize(rest);
            if tokens.len() >= 2 {
                return Line::Definition {
                    name: tokens[0].value.clone(),
                    target: tokens[1..].iter().map(|t| t.value.as_str()).collect::<Vec<_>>().join(" "),
                    notation: self.notation(),
                };
            }
            // Malformed definition — treat as comment
            return Line::Comment {
                text: raw.to_string(),
            };
        }

        let tokens = tokenize(trimmed);
        if tokens.is_empty() {
            return Line::Empty;
        }

        // Type annotation: first token ends with `:`
        if tokens[0].value.ends_with(':') && !tokens[0].quoted {
            let name = tokens[0].value.trim_end_matches(':').to_string();
            let type_expr = tokens[1..].iter().map(|t| t.value.as_str()).collect::<Vec<_>>().join(" ");
            return Line::TypeAnnotation { name, type_expr };
        }

        // Check for kerai.* directive
        if tokens[0].value.starts_with("kerai.") && !tokens[0].quoted {
            let directive_name = tokens[0].value.clone();
            let args: Vec<String> = tokens[1..].iter().map(|t| t.value.clone()).collect();

            // Apply side effect: notation mode change
            match directive_name.as_str() {
                "kerai.prefix" => self.set_notation(Notation::Prefix),
                "kerai.infix" => self.set_notation(Notation::Infix),
                "kerai.postfix" => self.set_notation(Notation::Postfix),
                _ => {}
            }

            return Line::Directive {
                name: directive_name,
                args,
            };
        }

        // Function call — interpretation depends on notation mode
        let notation = self.notation();
        let values: Vec<String> = tokens.into_iter().map(|t| t.value).collect();

        if values.len() == 1 {
            return Line::Call {
                function: values[0].clone(),
                args: vec![],
                notation,
            };
        }

        let (function, args) = match notation {
            Notation::Prefix => {
                let function = values[0].clone();
                let args = values[1..].to_vec();
                (function, args)
            }
            Notation::Infix => {
                // Second token is function, rest are args (first token becomes first arg)
                let function = values[1].clone();
                let mut args = vec![values[0].clone()];
                args.extend_from_slice(&values[2..]);
                (function, args)
            }
            Notation::Postfix => {
                let function = values.last().unwrap().clone();
                let args = values[..values.len() - 1].to_vec();
                (function, args)
            }
        };

        Line::Call {
            function,
            args,
            notation,
        }
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_comment_lines() {
        let mut parser = Parser::new();
        let doc = parser.parse("# a comment\n\n// another comment\n");
        assert_eq!(doc.lines.len(), 3);
        assert!(matches!(&doc.lines[0], Line::Comment { text } if text == "# a comment"));
        assert!(matches!(&doc.lines[1], Line::Empty));
        assert!(matches!(&doc.lines[2], Line::Comment { text } if text == "// another comment"));
    }

    #[test]
    fn definition_line() {
        let mut parser = Parser::new();
        let doc = parser.parse(":pg postgres\n");
        assert_eq!(doc.lines.len(), 1);
        match &doc.lines[0] {
            Line::Definition { name, target, notation } => {
                assert_eq!(name, "pg");
                assert_eq!(target, "postgres");
                assert_eq!(*notation, Notation::Prefix);
            }
            other => panic!("expected Definition, got {other:?}"),
        }
    }

    #[test]
    fn type_annotation() {
        let mut parser = Parser::new();
        let doc = parser.parse("name: String\n");
        assert_eq!(doc.lines.len(), 1);
        match &doc.lines[0] {
            Line::TypeAnnotation { name, type_expr } => {
                assert_eq!(name, "name");
                assert_eq!(type_expr, "String");
            }
            other => panic!("expected TypeAnnotation, got {other:?}"),
        }
    }

    #[test]
    fn prefix_call() {
        let mut parser = Parser::new();
        let doc = parser.parse("postgres.global.connection localhost\n");
        assert_eq!(doc.lines.len(), 1);
        match &doc.lines[0] {
            Line::Call { function, args, notation } => {
                assert_eq!(function, "postgres.global.connection");
                assert_eq!(args, &["localhost"]);
                assert_eq!(*notation, Notation::Prefix);
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn infix_mode() {
        let mut parser = Parser::new();
        let doc = parser.parse("kerai.infix\na b c\n");
        assert_eq!(doc.lines.len(), 2);
        assert!(matches!(&doc.lines[0], Line::Directive { name, .. } if name == "kerai.infix"));
        match &doc.lines[1] {
            Line::Call { function, args, notation } => {
                assert_eq!(function, "b");
                assert_eq!(args, &["a", "c"]);
                assert_eq!(*notation, Notation::Infix);
            }
            other => panic!("expected infix Call, got {other:?}"),
        }
    }

    #[test]
    fn postfix_mode() {
        let mut parser = Parser::new();
        let doc = parser.parse("kerai.postfix\na b c\n");
        assert_eq!(doc.lines.len(), 2);
        match &doc.lines[1] {
            Line::Call { function, args, notation } => {
                assert_eq!(function, "c");
                assert_eq!(args, &["a", "b"]);
                assert_eq!(*notation, Notation::Postfix);
            }
            other => panic!("expected postfix Call, got {other:?}"),
        }
    }

    #[test]
    fn notation_switch_midfile() {
        let mut parser = Parser::new();
        let doc = parser.parse("a b\nkerai.infix\nc d\nkerai.prefix\ne f\n");
        // Line 0: prefix call a(b)
        assert!(matches!(&doc.lines[0], Line::Call { function, notation, .. } if function == "a" && *notation == Notation::Prefix));
        // Line 1: directive
        assert!(matches!(&doc.lines[1], Line::Directive { .. }));
        // Line 2: infix call d(c)
        assert!(matches!(&doc.lines[2], Line::Call { function, notation, .. } if function == "d" && *notation == Notation::Infix));
        // Line 3: directive
        assert!(matches!(&doc.lines[3], Line::Directive { .. }));
        // Line 4: prefix call e(f)
        assert!(matches!(&doc.lines[4], Line::Call { function, notation, .. } if function == "e" && *notation == Notation::Prefix));
    }

    #[test]
    fn single_token_call() {
        let mut parser = Parser::new();
        let doc = parser.parse("ping\n");
        match &doc.lines[0] {
            Line::Call { function, args, .. } => {
                assert_eq!(function, "ping");
                assert!(args.is_empty());
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn directive_with_args() {
        let mut parser = Parser::new();
        let doc = parser.parse("kerai.custom foo bar\n");
        match &doc.lines[0] {
            Line::Directive { name, args } => {
                assert_eq!(name, "kerai.custom");
                assert_eq!(args, &["foo", "bar"]);
            }
            other => panic!("expected Directive, got {other:?}"),
        }
    }

    #[test]
    fn quoted_args_in_call() {
        let mut parser = Parser::new();
        let doc = parser.parse(r#"postgres.global.connection "postgres://localhost/kerai""#);
        match &doc.lines[0] {
            Line::Call { function, args, .. } => {
                assert_eq!(function, "postgres.global.connection");
                assert_eq!(args, &["postgres://localhost/kerai"]);
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn definition_with_multiple_words() {
        let mut parser = Parser::new();
        let doc = parser.parse(":alias some target value\n");
        match &doc.lines[0] {
            Line::Definition { name, target, .. } => {
                assert_eq!(name, "alias");
                assert_eq!(target, "some target value");
            }
            other => panic!("expected Definition, got {other:?}"),
        }
    }

    #[test]
    fn default_notation_tracks_final_state() {
        let mut parser = Parser::new();
        let doc = parser.parse("kerai.infix\n");
        assert_eq!(doc.default_notation, Notation::Infix);
    }
}
