use std::fmt;

use super::expr::Expr;

/// Result of evaluating an expression.
pub enum Value {
    Int(i64),
    Float(f64),
    Str(String),
    List(Vec<Value>),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Float(v) => {
                let s = format!("{v}");
                // Strip trailing ".0" for integer-valued floats
                if s.ends_with(".0") {
                    write!(f, "{}", &s[..s.len() - 2])
                } else {
                    write!(f, "{s}")
                }
            }
            Value::Str(s) => write!(f, "{s}"),
            Value::List(vs) => {
                let inner: Vec<String> = vs.iter().map(|v| v.to_string()).collect();
                write!(f, "[{}]", inner.join(" "))
            }
        }
    }
}

/// Evaluate an expression tree to a value.
pub fn eval(expr: &Expr) -> Value {
    match expr {
        Expr::Atom(s) => parse_atom(s),
        Expr::List(elements) => Value::List(elements.iter().map(eval).collect()),
        Expr::Apply { function, args } => eval_apply(function, args),
    }
}

/// Try to parse an atom as a number, falling back to string.
fn parse_atom(s: &str) -> Value {
    // Hex literal
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        if let Ok(n) = i64::from_str_radix(hex, 16) {
            return Value::Int(n);
        }
    }
    if let Ok(n) = s.parse::<i64>() {
        return Value::Int(n);
    }
    if let Ok(f) = s.parse::<f64>() {
        return Value::Float(f);
    }
    Value::Str(s.to_string())
}

/// Evaluate a function application.
fn eval_apply(function: &str, args: &[Expr]) -> Value {
    if args.len() == 2 && is_binary_op(function) {
        let lhs = eval(&args[0]);
        let rhs = eval(&args[1]);
        if let Some(result) = eval_binary_op(function, &lhs, &rhs) {
            return result;
        }
    }
    // Unknown function or non-numeric args — render as expression string
    Value::Str(render_apply(function, args))
}

fn is_binary_op(s: &str) -> bool {
    matches!(s, "+" | "-" | "*" | "/" | "%")
}

/// Evaluate a binary operation on two numeric values.
fn eval_binary_op(op: &str, lhs: &Value, rhs: &Value) -> Option<Value> {
    match (lhs, rhs) {
        (Value::Int(a), Value::Int(b)) => Some(int_op(op, *a, *b)),
        (Value::Int(a), Value::Float(b)) => Some(float_op(op, *a as f64, *b)),
        (Value::Float(a), Value::Int(b)) => Some(float_op(op, *a, *b as f64)),
        (Value::Float(a), Value::Float(b)) => Some(float_op(op, *a, *b)),
        _ => None,
    }
}

fn int_op(op: &str, a: i64, b: i64) -> Value {
    match op {
        "+" => Value::Int(a.wrapping_add(b)),
        "-" => Value::Int(a.wrapping_sub(b)),
        "*" => Value::Int(a.wrapping_mul(b)),
        "/" => {
            if b == 0 {
                Value::Str("division by zero".to_string())
            } else {
                Value::Int(a / b)
            }
        }
        "%" => {
            if b == 0 {
                Value::Str("division by zero".to_string())
            } else {
                Value::Int(a % b)
            }
        }
        _ => Value::Str(format!("{a} {op} {b}")),
    }
}

fn float_op(op: &str, a: f64, b: f64) -> Value {
    match op {
        "+" => Value::Float(a + b),
        "-" => Value::Float(a - b),
        "*" => Value::Float(a * b),
        "/" => {
            if b == 0.0 {
                Value::Str("division by zero".to_string())
            } else {
                Value::Float(a / b)
            }
        }
        "%" => {
            if b == 0.0 {
                Value::Str("division by zero".to_string())
            } else {
                Value::Float(a % b)
            }
        }
        _ => Value::Str(format!("{a} {op} {b}")),
    }
}

/// Render an Apply expression back to a readable string.
fn render_apply(function: &str, args: &[Expr]) -> String {
    if args.is_empty() {
        return function.to_string();
    }
    let rendered: Vec<String> = args.iter().map(render_expr).collect();
    format!("({function} {})", rendered.join(" "))
}

fn render_expr(expr: &Expr) -> String {
    match expr {
        Expr::Atom(s) => s.clone(),
        Expr::Apply { function, args } => render_apply(function, args),
        Expr::List(elements) => {
            let inner: Vec<String> = elements.iter().map(render_expr).collect();
            format!("[{}]", inner.join(" "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eval_int_atom() {
        let v = eval(&Expr::Atom("42".into()));
        assert_eq!(v.to_string(), "42");
    }

    #[test]
    fn eval_float_atom() {
        let v = eval(&Expr::Atom("3.14".into()));
        assert_eq!(v.to_string(), "3.14");
    }

    #[test]
    fn eval_hex_atom() {
        let v = eval(&Expr::Atom("0xFF".into()));
        assert_eq!(v.to_string(), "255");
    }

    #[test]
    fn eval_string_atom() {
        let v = eval(&Expr::Atom("hello".into()));
        assert_eq!(v.to_string(), "hello");
    }

    #[test]
    fn eval_int_add() {
        let expr = Expr::Apply {
            function: "+".into(),
            args: vec![Expr::Atom("3".into()), Expr::Atom("4".into())],
        };
        assert_eq!(eval(&expr).to_string(), "7");
    }

    #[test]
    fn eval_int_div() {
        let expr = Expr::Apply {
            function: "/".into(),
            args: vec![Expr::Atom("10".into()), Expr::Atom("3".into())],
        };
        assert_eq!(eval(&expr).to_string(), "3");
    }

    #[test]
    fn eval_float_div() {
        let expr = Expr::Apply {
            function: "/".into(),
            args: vec![Expr::Atom("10.0".into()), Expr::Atom("3".into())],
        };
        let result = eval(&expr).to_string();
        assert!(result.starts_with("3.333333333333333"));
    }

    #[test]
    fn eval_div_by_zero() {
        let expr = Expr::Apply {
            function: "/".into(),
            args: vec![Expr::Atom("1".into()), Expr::Atom("0".into())],
        };
        assert_eq!(eval(&expr).to_string(), "division by zero");
    }

    #[test]
    fn eval_nested() {
        // 1 + (2 * 3) = 7
        let expr = Expr::Apply {
            function: "+".into(),
            args: vec![
                Expr::Atom("1".into()),
                Expr::Apply {
                    function: "*".into(),
                    args: vec![Expr::Atom("2".into()), Expr::Atom("3".into())],
                },
            ],
        };
        assert_eq!(eval(&expr).to_string(), "7");
    }

    #[test]
    fn eval_list() {
        let expr = Expr::List(vec![
            Expr::Atom("1".into()),
            Expr::Atom("2".into()),
            Expr::Atom("3".into()),
        ]);
        assert_eq!(eval(&expr).to_string(), "[1 2 3]");
    }

    #[test]
    fn eval_unknown_function() {
        let expr = Expr::Apply {
            function: "foo".into(),
            args: vec![Expr::Atom("bar".into())],
        };
        assert_eq!(eval(&expr).to_string(), "(foo bar)");
    }

    #[test]
    fn eval_integer_valued_float() {
        let v = eval(&Expr::Atom("4.0".into()));
        assert_eq!(v.to_string(), "4");
    }

    #[test]
    fn eval_float_add_integer_result() {
        let expr = Expr::Apply {
            function: "+".into(),
            args: vec![Expr::Atom("1.5".into()), Expr::Atom("2.5".into())],
        };
        assert_eq!(eval(&expr).to_string(), "4");
    }

    #[test]
    fn eval_modulo() {
        let expr = Expr::Apply {
            function: "%".into(),
            args: vec![Expr::Atom("10".into()), Expr::Atom("3".into())],
        };
        assert_eq!(eval(&expr).to_string(), "1");
    }

    #[test]
    fn eval_string_op_fallback() {
        // Adding non-numeric values falls back to string rendering
        let expr = Expr::Apply {
            function: "+".into(),
            args: vec![Expr::Atom("hello".into()), Expr::Atom("world".into())],
        };
        assert_eq!(eval(&expr).to_string(), "(+ hello world)");
    }
}
