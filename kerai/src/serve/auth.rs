use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use super::db::Pool;

#[derive(Serialize)]
pub struct SessionInfo {
    pub user_id: String,
    pub workspace_id: String,
    pub workspace_name: String,
    pub handle: Option<String>,
    pub auth_provider: String,
    pub token: String,
}

/// GET /auth/session — Return current session info or create anonymous session.
pub async fn get_session(
    State(pool): State<Arc<Pool>>,
    headers: HeaderMap,
) -> Result<Json<SessionInfo>, (StatusCode, String)> {
    let client = pool.get().await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    // Check for existing session cookie
    if let Some(token) = extract_session_token(&headers) {
        if let Some(info) = lookup_session(&client, &token).await? {
            return Ok(Json(info));
        }
    }

    // No valid session — create anonymous user + workspace + session
    let user_id: Uuid = client
        .query_one(
            "INSERT INTO kerai.users (auth_provider) VALUES ('anonymous') RETURNING id",
            &[],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .get(0);

    // Generate a short anonymous workspace name from user_id
    let ws_name = format!("anon-{}", &user_id.to_string()[..8]);

    let workspace_id: Uuid = client
        .query_one(
            "INSERT INTO kerai.workspaces (user_id, name, is_active, is_anonymous) \
             VALUES ($1, $2, true, true) RETURNING id",
            &[&user_id, &ws_name],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .get(0);

    // Generate session token
    let token = generate_token();

    client
        .execute(
            "INSERT INTO kerai.sessions (user_id, workspace_id, token) VALUES ($1, $2, $3)",
            &[&user_id, &workspace_id, &token],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(SessionInfo {
        user_id: user_id.to_string(),
        workspace_id: workspace_id.to_string(),
        workspace_name: ws_name,
        handle: None,
        auth_provider: "anonymous".into(),
        token,
    }))
}

#[derive(Deserialize)]
pub struct BskyStartRequest {
    pub handle: Option<String>,
}

/// POST /auth/bsky/start — Begin AT Protocol OAuth. Returns authorize URL.
/// For now, returns a placeholder; full OAuth implementation in Phase 6.
pub async fn bsky_start(
    State(_pool): State<Arc<Pool>>,
    Json(req): Json<BskyStartRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let handle = req.handle.unwrap_or_else(|| "user.bsky.social".into());

    // TODO: Implement full AT Protocol OAuth flow:
    // 1. Resolve handle → DID
    // 2. Discover authorization server
    // 3. PAR → get request_uri
    // 4. Return authorization URL

    Ok(Json(json!({
        "status": "not_implemented",
        "message": format!("Bluesky OAuth for {} coming in Phase 6", handle),
    })))
}

/// GET /auth/bsky/callback — Handle OAuth callback.
/// Placeholder for Phase 6 implementation.
pub async fn bsky_callback(
    State(_pool): State<Arc<Pool>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    Ok(Json(json!({
        "status": "not_implemented",
        "message": "OAuth callback not yet implemented",
    })))
}

/// POST /auth/logout — Clear session.
pub async fn logout(
    State(pool): State<Arc<Pool>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let client = pool.get().await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    if let Some(token) = extract_session_token(&headers) {
        client
            .execute(
                "DELETE FROM kerai.sessions WHERE token = $1",
                &[&token],
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    Ok(Json(json!({"status": "logged_out"})))
}

/// Extract session token from Cookie header.
fn extract_session_token(headers: &HeaderMap) -> Option<String> {
    let cookie_header = headers.get("cookie")?.to_str().ok()?;
    for pair in cookie_header.split(';') {
        let pair = pair.trim();
        if let Some(value) = pair.strip_prefix("kerai_session=") {
            let token = value.trim();
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }
    None
}

/// Look up a session by token and return session info.
async fn lookup_session(
    client: &tokio_postgres::Client,
    token: &str,
) -> Result<Option<SessionInfo>, (StatusCode, String)> {
    let row = client
        .query_opt(
            "SELECT s.user_id, s.workspace_id, w.name, u.handle, u.auth_provider, s.token \
             FROM kerai.sessions s \
             JOIN kerai.users u ON u.id = s.user_id \
             JOIN kerai.workspaces w ON w.id = s.workspace_id \
             WHERE s.token = $1 AND s.expires_at > now()",
            &[&token],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(row.map(|r| SessionInfo {
        user_id: r.get::<_, Uuid>(0).to_string(),
        workspace_id: r.get::<_, Uuid>(1).to_string(),
        workspace_name: r.get::<_, String>(2),
        handle: r.get::<_, Option<String>>(3),
        auth_provider: r.get::<_, String>(4),
        token: r.get::<_, String>(5),
    }))
}

/// Generate a random session token (hex-encoded 32 bytes).
fn generate_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
    hex_encode(&bytes)
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Look up the session for a given token string. Used by eval route.
pub async fn resolve_session(
    pool: &Pool,
    token: &str,
) -> Result<(Uuid, Uuid), String> {
    let client = pool.get().await.map_err(|e| e.to_string())?;

    let row = client
        .query_opt(
            "SELECT user_id, workspace_id FROM kerai.sessions \
             WHERE token = $1 AND expires_at > now()",
            &[&token],
        )
        .await
        .map_err(|e| e.to_string())?
        .ok_or("invalid or expired session")?;

    let user_id: Uuid = row.get(0);
    let workspace_id: Uuid = row.get(1);
    Ok((user_id, workspace_id))
}
