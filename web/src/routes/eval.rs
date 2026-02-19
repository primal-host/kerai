use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use axum::Json;
use serde::{Deserialize, Serialize};

use kerai_cli::lang;

#[derive(Deserialize)]
pub struct EvalRequest {
    input: String,
    #[serde(default = "default_notation")]
    notation: String,
}

fn default_notation() -> String {
    "infix".to_string()
}

#[derive(Serialize)]
pub struct EvalResponse {
    output: String,
    error: bool,
}

pub async fn eval(Json(req): Json<EvalRequest>) -> (StatusCode, Json<EvalResponse>) {
    let notation = match req.notation.as_str() {
        "prefix" => lang::Notation::Prefix,
        "postfix" => lang::Notation::Postfix,
        _ => lang::Notation::Infix,
    };

    let expr = match lang::parse_expr(&req.input, notation) {
        Some(e) => e,
        None => {
            return (
                StatusCode::OK,
                Json(EvalResponse {
                    output: format!("could not parse: {}", req.input),
                    error: true,
                }),
            );
        }
    };

    let value = lang::eval::eval(&expr);
    match value {
        lang::eval::Value::Str(s) => (
            StatusCode::OK,
            Json(EvalResponse {
                output: s,
                error: true,
            }),
        ),
        _ => (
            StatusCode::OK,
            Json(EvalResponse {
                output: value.to_string(),
                error: false,
            }),
        ),
    }
}

pub async fn terminal_page() -> impl IntoResponse {
    Html(include_str!("../../terminal.html"))
}
