use axum::extract::State;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use super::super::db::Pool;

#[derive(Deserialize)]
pub struct PushRequest {
    pub content: String,
    pub label: Option<String>,
}

#[derive(Deserialize)]
pub struct ReplaceRequest {
    pub content: String,
}

/// GET /api/stack — peek at stack top
pub async fn stack_peek(
    State(pool): State<Arc<Pool>>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let client = pool.get().await.map_err(|e| {
        (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    let row = client
        .query_opt("SELECT kerai.stack_peek()", &[])
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let content = row.and_then(|r| r.get::<_, Option<String>>(0));
    Ok(Json(json!({ "content": content })))
}

/// GET /api/stack/list — list all stack entries
pub async fn stack_list(
    State(pool): State<Arc<Pool>>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let client = pool.get().await.map_err(|e| {
        (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    let rows = client
        .query(
            "SELECT position, label, preview, created_at FROM kerai.stack_list()",
            &[],
        )
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let entries: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "position": r.get::<_, i32>(0),
                "label": r.get::<_, String>(1),
                "preview": r.get::<_, String>(2),
                "createdAt": r.get::<_, String>(3),
            })
        })
        .collect();

    Ok(Json(json!(entries)))
}

/// POST /api/stack/push — push content onto stack
pub async fn stack_push(
    State(pool): State<Arc<Pool>>,
    Json(req): Json<PushRequest>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let client = pool.get().await.map_err(|e| {
        (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    let row = client
        .query_one(
            "SELECT kerai.stack_push($1, $2)",
            &[&req.content, &req.label],
        )
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?;

    let position: i32 = row.get(0);
    Ok(Json(json!({ "position": position })))
}

/// DELETE /api/stack — drop top entry
pub async fn stack_drop(
    State(pool): State<Arc<Pool>>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let client = pool.get().await.map_err(|e| {
        (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    let row = client
        .query_one("SELECT kerai.stack_drop()", &[])
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let status: String = row.get(0);
    Ok(Json(json!({ "status": status })))
}

/// DELETE /api/stack/all — clear entire stack
pub async fn stack_clear(
    State(pool): State<Arc<Pool>>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let client = pool.get().await.map_err(|e| {
        (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    let row = client
        .query_one("SELECT kerai.stack_clear()", &[])
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let cleared: i32 = row.get(0);
    Ok(Json(json!({ "cleared": cleared })))
}

/// PUT /api/stack — replace top entry content
pub async fn stack_replace(
    State(pool): State<Arc<Pool>>,
    Json(req): Json<ReplaceRequest>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let client = pool.get().await.map_err(|e| {
        (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    let row = client
        .query_one("SELECT kerai.stack_replace($1)", &[&req.content])
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let status: String = row.get(0);
    Ok(Json(json!({ "status": status })))
}

/// POST /api/init/pull — render preferences and push onto stack
pub async fn init_pull(
    State(pool): State<Arc<Pool>>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let client = pool.get().await.map_err(|e| {
        (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    let row = client
        .query_one("SELECT kerai.pull_init()", &[])
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let status: String = row.get(0);
    Ok(Json(json!({ "status": status })))
}

/// POST /api/init/push — parse stack top and apply to preferences
pub async fn init_push(
    State(pool): State<Arc<Pool>>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let client = pool.get().await.map_err(|e| {
        (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    let row = client
        .query_one("SELECT kerai.push_init()", &[])
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let result: String = row.get(0);
    // push_init returns JSON, parse it so we return proper JSON not a string
    let parsed: Value = serde_json::from_str(&result).unwrap_or(json!({ "result": result }));
    Ok(Json(parsed))
}

/// GET /api/init/diff — show what push would change
pub async fn init_diff(
    State(pool): State<Arc<Pool>>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let client = pool.get().await.map_err(|e| {
        (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    let row = client
        .query_one("SELECT kerai.diff_init()", &[])
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let result: String = row.get(0);
    let parsed: Value = serde_json::from_str(&result).unwrap_or(json!([]));
    Ok(Json(parsed))
}
