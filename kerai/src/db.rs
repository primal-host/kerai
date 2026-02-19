use postgres::{Client, NoTls};

use crate::config::Profile;

/// Connect to Postgres. `db_override` (from --db flag) takes highest priority,
/// then the profile's connection string.
pub fn connect(profile: &Profile, db_override: Option<&str>) -> Result<Client, String> {
    let conn_str = db_override
        .or(profile.connection.as_deref())
        .ok_or("No connection string. Use --db or set one in .kerai/config.toml")?;

    Client::connect(conn_str, NoTls).map_err(|e| format!("Connection failed: {e}"))
}

/// Ensure ltree and kerai extensions are loaded.
pub fn ensure_extension(client: &mut Client) -> Result<(), String> {
    client
        .batch_execute("CREATE EXTENSION IF NOT EXISTS ltree; CREATE EXTENSION IF NOT EXISTS kerai CASCADE;")
        .map_err(|e| format!("Failed to create extension: {e}"))
}
