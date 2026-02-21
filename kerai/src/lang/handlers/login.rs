use crate::lang::machine::Machine;
use crate::lang::ptr::Ptr;

/// Push the login library marker onto the stack.
pub fn login_lib(m: &mut Machine) -> Result<(), String> {
    m.push(Ptr::library("login"));
    Ok(())
}

/// `login bsky` — initiate Bluesky OAuth.
/// Pushes an auth_pending Ptr that the web UI detects and redirects to.
/// The actual OAuth URL generation happens in the serve auth layer.
pub fn login_bsky(m: &mut Machine) -> Result<(), String> {
    m.push(Ptr {
        kind: "auth_pending_request".into(),
        ref_id: "bsky".into(),
        meta: serde_json::Value::Null,
        id: 0,
    });
    Ok(())
}
