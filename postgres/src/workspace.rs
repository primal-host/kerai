use pgrx::prelude::*;

use crate::sql::{sql_jsonb, sql_text, sql_uuid};

/// Push a typed item onto a workspace stack. Returns the stable rowid.
#[pg_extern]
fn ws_stack_push(workspace_id: pgrx::Uuid, kind: &str, ref_id: &str, meta: pgrx::JsonB) -> i64 {
    let ws = workspace_id.to_string();
    let pos = Spi::get_one::<i32>(&format!(
        "SELECT COALESCE(MAX(position), -1) + 1 FROM kerai.stack_items \
         WHERE workspace_id = {}",
        sql_uuid(&ws),
    ))
    .unwrap()
    .unwrap_or(0);

    let meta_sql = sql_jsonb(&meta.0);

    let id = Spi::get_one::<i64>(&format!(
        "INSERT INTO kerai.stack_items (workspace_id, position, kind, ref_id, meta) \
         VALUES ({}, {pos}, {}, {}, {}) \
         RETURNING id",
        sql_uuid(&ws),
        sql_text(kind),
        sql_text(ref_id),
        meta_sql,
    ))
    .expect("ws_stack_push SPI failed")
    .expect("ws_stack_push returned no id");

    id
}

/// Pop the top item from a workspace stack. Returns JSON with {id, kind, ref_id, meta}.
#[pg_extern]
fn ws_stack_pop(workspace_id: pgrx::Uuid) -> Option<pgrx::JsonB> {
    let ws = workspace_id.to_string();

    let result = Spi::get_one::<pgrx::JsonB>(&format!(
        "WITH top AS ( \
             DELETE FROM kerai.stack_items \
             WHERE id = ( \
                 SELECT id FROM kerai.stack_items \
                 WHERE workspace_id = {} \
                 ORDER BY position DESC LIMIT 1 \
             ) \
             RETURNING id, kind, ref_id, meta \
         ) SELECT jsonb_build_object('id', id, 'kind', kind, 'ref_id', ref_id, 'meta', meta) FROM top",
        sql_uuid(&ws),
    ))
    .unwrap_or(None);

    result
}

/// List all items in a workspace stack, ordered by position descending.
#[pg_extern]
fn ws_stack_list(
    workspace_id: pgrx::Uuid,
) -> TableIterator<
    'static,
    (
        name!(id, i64),
        name!(position, i32),
        name!(kind, String),
        name!(ref_id, String),
        name!(meta, pgrx::JsonB),
    ),
> {
    let ws = workspace_id.to_string();

    let query = format!(
        "SELECT id, position, kind, ref_id, meta \
         FROM kerai.stack_items \
         WHERE workspace_id = {} \
         ORDER BY position DESC",
        sql_uuid(&ws),
    );

    let mut rows = Vec::new();
    Spi::connect(|client| {
        let tup_table = client.select(&query, None, &[]).unwrap();
        for row in tup_table {
            let id: i64 = row.get_by_name("id").unwrap().unwrap_or(0);
            let position: i32 = row.get_by_name("position").unwrap().unwrap_or(0);
            let kind: String = row.get_by_name("kind").unwrap().unwrap_or_default();
            let ref_id: String = row.get_by_name("ref_id").unwrap().unwrap_or_default();
            let meta: pgrx::JsonB = row
                .get_by_name("meta")
                .unwrap()
                .unwrap_or(pgrx::JsonB(serde_json::json!({})));
            rows.push((id, position, kind, ref_id, meta));
        }
    });

    TableIterator::new(rows)
}

/// Drop a specific item by rowid. Returns true if deleted.
#[pg_extern]
fn ws_stack_drop_by_id(item_id: i64) -> bool {
    let deleted = Spi::get_one::<bool>(&format!(
        "WITH d AS ( \
             DELETE FROM kerai.stack_items WHERE id = {item_id} RETURNING id \
         ) SELECT EXISTS(SELECT 1 FROM d)",
    ))
    .unwrap_or(Some(false))
    .unwrap_or(false);

    deleted
}

/// Duplicate an item by rowid, pushing the copy to the top. Returns new rowid.
#[pg_extern]
fn ws_stack_dup_by_id(item_id: i64, workspace_id: pgrx::Uuid) -> Option<i64> {
    let ws = workspace_id.to_string();

    let pos = Spi::get_one::<i32>(&format!(
        "SELECT COALESCE(MAX(position), -1) + 1 FROM kerai.stack_items \
         WHERE workspace_id = {}",
        sql_uuid(&ws),
    ))
    .unwrap()
    .unwrap_or(0);

    let new_id = Spi::get_one::<i64>(&format!(
        "INSERT INTO kerai.stack_items (workspace_id, position, kind, ref_id, meta) \
         SELECT workspace_id, {pos}, kind, ref_id, meta \
         FROM kerai.stack_items WHERE id = {item_id} \
         RETURNING id",
    ))
    .unwrap_or(None);

    new_id
}

/// Clear all items from a workspace stack. Returns the count cleared.
#[pg_extern]
fn ws_stack_clear(workspace_id: pgrx::Uuid) -> i32 {
    let ws = workspace_id.to_string();

    let count = Spi::get_one::<i64>(&format!(
        "WITH d AS ( \
             DELETE FROM kerai.stack_items WHERE workspace_id = {} RETURNING id \
         ) SELECT COUNT(*) FROM d",
        sql_uuid(&ws),
    ))
    .unwrap()
    .unwrap_or(0);

    count as i32
}

/// Return the number of items in a workspace stack.
#[pg_extern]
fn ws_stack_depth(workspace_id: pgrx::Uuid) -> i32 {
    let ws = workspace_id.to_string();

    Spi::get_one::<i64>(&format!(
        "SELECT COUNT(*) FROM kerai.stack_items WHERE workspace_id = {}",
        sql_uuid(&ws),
    ))
    .unwrap()
    .unwrap_or(0) as i32
}

/// Create an anonymous user and workspace. Returns (user_id, workspace_id).
#[pg_extern]
fn ws_ensure_anonymous() -> TableIterator<
    'static,
    (name!(user_id, pgrx::Uuid), name!(workspace_id, pgrx::Uuid)),
> {
    let result = Spi::get_two::<pgrx::Uuid, pgrx::Uuid>(
        "WITH new_user AS ( \
             INSERT INTO kerai.users (auth_provider) VALUES ('anonymous') \
             RETURNING id \
         ), new_ws AS ( \
             INSERT INTO kerai.workspaces (user_id, name, is_active, is_anonymous) \
             SELECT id, 'default', true, true FROM new_user \
             RETURNING id \
         ) SELECT new_user.id, new_ws.id FROM new_user, new_ws",
    );

    match result {
        Ok((Some(user_id), Some(workspace_id))) => {
            TableIterator::new(vec![(user_id, workspace_id)])
        }
        _ => TableIterator::new(vec![]),
    }
}

/// Create a named workspace for a user. Returns the workspace id.
#[pg_extern]
fn ws_create(user_id: pgrx::Uuid, name: &str) -> Option<pgrx::Uuid> {
    let uid = user_id.to_string();

    Spi::get_one::<pgrx::Uuid>(&format!(
        "INSERT INTO kerai.workspaces (user_id, name) VALUES ({}, {}) RETURNING id",
        sql_uuid(&uid),
        sql_text(name),
    ))
    .unwrap_or(None)
}

/// List all workspaces for a user with item counts.
#[pg_extern]
fn ws_list(
    user_id: pgrx::Uuid,
) -> TableIterator<
    'static,
    (
        name!(id, pgrx::Uuid),
        name!(name, String),
        name!(is_active, bool),
        name!(item_count, i32),
        name!(updated_at, String),
    ),
> {
    let uid = user_id.to_string();

    let query = format!(
        "SELECT w.id, w.name, w.is_active, \
         COALESCE((SELECT COUNT(*)::int FROM kerai.stack_items si WHERE si.workspace_id = w.id), 0) AS item_count, \
         w.updated_at::text \
         FROM kerai.workspaces w \
         WHERE w.user_id = {} \
         ORDER BY w.updated_at DESC",
        sql_uuid(&uid),
    );

    let mut rows = Vec::new();
    Spi::connect(|client| {
        let tup_table = client.select(&query, None, &[]).unwrap();
        for row in tup_table {
            let id: pgrx::Uuid = row.get_by_name("id").unwrap().unwrap();
            let name: String = row.get_by_name("name").unwrap().unwrap_or_default();
            let is_active: bool = row.get_by_name("is_active").unwrap().unwrap_or(false);
            let item_count: i32 = row.get_by_name("item_count").unwrap().unwrap_or(0);
            let updated_at: String = row.get_by_name("updated_at").unwrap().unwrap_or_default();
            rows.push((id, name, is_active, item_count, updated_at));
        }
    });

    TableIterator::new(rows)
}

/// Set a workspace as active, deactivating others for the same user. Returns true on success.
#[pg_extern]
fn ws_activate(workspace_id: pgrx::Uuid) -> bool {
    let ws = workspace_id.to_string();

    let success = Spi::get_one::<bool>(&format!(
        "WITH target AS ( \
             SELECT user_id FROM kerai.workspaces WHERE id = {} \
         ), deactivate AS ( \
             UPDATE kerai.workspaces SET is_active = false \
             WHERE user_id = (SELECT user_id FROM target) AND is_active = true \
         ), activate AS ( \
             UPDATE kerai.workspaces SET is_active = true, updated_at = now() \
             WHERE id = {} \
             RETURNING id \
         ) SELECT EXISTS(SELECT 1 FROM activate)",
        sql_uuid(&ws),
        sql_uuid(&ws),
    ))
    .unwrap_or(Some(false))
    .unwrap_or(false);

    success
}
