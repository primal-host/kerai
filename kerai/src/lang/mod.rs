pub mod ast;
mod parser;
pub mod token;

use std::fs;
use std::path::Path;

pub use ast::{Document, Line, Notation};

/// Parse kerai language source text into a Document.
pub fn parse(source: &str) -> Document {
    let mut parser = parser::Parser::new();
    parser.parse(source)
}

/// Parse a `.kerai` file into a Document.
pub fn parse_file(path: &Path) -> Result<Document, String> {
    let content =
        fs::read_to_string(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    Ok(parse(&content))
}

/// Extract definitions from a document as `(name, target)` pairs.
pub fn definitions(doc: &Document) -> Vec<(&str, &str)> {
    doc.lines
        .iter()
        .filter_map(|line| match line {
            Line::Definition { name, target, .. } => Some((name.as_str(), target.as_str())),
            _ => None,
        })
        .collect()
}

/// Extract function calls from a document as `(function, args)` pairs.
pub fn calls(doc: &Document) -> Vec<(&str, Vec<&str>)> {
    doc.lines
        .iter()
        .filter_map(|line| match line {
            Line::Call { function, args, .. } => {
                Some((function.as_str(), args.iter().map(|s| s.as_str()).collect()))
            }
            _ => None,
        })
        .collect()
}

/// Render a single line back to source text.
pub fn render_line(line: &Line) -> String {
    match line {
        Line::Empty => String::new(),
        Line::Comment { text } => text.clone(),
        Line::Definition { name, target, .. } => format!(":{name} {target}"),
        Line::TypeAnnotation { name, type_expr } => format!("{name}: {type_expr}"),
        Line::Call {
            function, args, ..
        } => {
            if args.is_empty() {
                function.clone()
            } else {
                format!("{function} {}", args.join(" "))
            }
        }
        Line::Directive { name, args } => {
            if args.is_empty() {
                name.clone()
            } else {
                format!("{name} {}", args.join(" "))
            }
        }
    }
}

/// Render a document back to source text (for round-tripping).
pub fn render(doc: &Document) -> String {
    let lines: Vec<String> = doc.lines.iter().map(render_line).collect();
    let mut output = lines.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_aliases_file() {
        let source = "# common aliases for kerai libraries\n:pg postgres\n";
        let doc = parse(source);
        assert_eq!(doc.lines.len(), 2);

        let defs = definitions(&doc);
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0], ("pg", "postgres"));
    }

    #[test]
    fn parse_kerai_config_file() {
        let source = "\
# kerai-controlled configuration — do not hand-edit
# syntax: :name target (definition) | name arg (function call) | name: type (reserved)
postgres.global.connection postgres://localhost/kerai
";
        let doc = parse(source);
        assert_eq!(doc.lines.len(), 3);

        let cs = calls(&doc);
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].0, "postgres.global.connection");
        assert_eq!(cs[0].1, vec!["postgres://localhost/kerai"]);
    }

    #[test]
    fn definitions_extracts_only_definitions() {
        let doc = parse(":a b\nfoo bar\n:c d\n# comment\n");
        let defs = definitions(&doc);
        assert_eq!(defs, vec![("a", "b"), ("c", "d")]);
    }

    #[test]
    fn calls_extracts_only_calls() {
        let doc = parse(":a b\nfoo bar baz\n# comment\nping\n");
        let cs = calls(&doc);
        assert_eq!(cs.len(), 2);
        assert_eq!(cs[0], ("foo", vec!["bar", "baz"]));
        assert_eq!(cs[1], ("ping", Vec::<&str>::new()));
    }

    #[test]
    fn round_trip_preserves_structure() {
        let source = "\
# comment line
:pg postgres

postgres.global.connection localhost
";
        let doc = parse(source);
        let rendered = render(&doc);
        assert_eq!(rendered, source);
    }

    #[test]
    fn empty_document() {
        let doc = parse("");
        assert!(doc.lines.is_empty());
        assert_eq!(doc.default_notation, Notation::Prefix);
    }

    #[test]
    fn notation_modes_all_work() {
        let source = "\
a b c
kerai.infix
a b c
kerai.postfix
a b c
";
        let doc = parse(source);
        let cs = calls(&doc);
        assert_eq!(cs.len(), 3);
        // prefix: a(b, c)
        assert_eq!(cs[0], ("a", vec!["b", "c"]));
        // infix: b(a, c)
        assert_eq!(cs[1], ("b", vec!["a", "c"]));
        // postfix: c(a, b)
        assert_eq!(cs[2], ("c", vec!["a", "b"]));
    }
}
