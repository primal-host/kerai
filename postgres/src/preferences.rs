use pgrx::prelude::*;

use crate::sql::{sql_escape, sql_text};

/// Get a preference value for the self instance.
#[pg_extern]
fn get_preference(category: &str, key: &str) -> Option<String> {
    Spi::get_one(&format!(
        "SELECT value FROM kerai.preferences \
         WHERE instance_id = (SELECT id FROM kerai.instances WHERE is_self = true) \
         AND category = {} AND key = {}",
        sql_text(category),
        sql_text(key),
    ))
    .unwrap_or(None)
}

/// Set (upsert) a preference for the self instance.
#[pg_extern]
fn set_preference(category: &str, key: &str, value: &str) -> &'static str {
    Spi::run(&format!(
        "INSERT INTO kerai.preferences (instance_id, category, key, value) \
         VALUES (\
             (SELECT id FROM kerai.instances WHERE is_self = true), \
             {}, {}, {}\
         ) \
         ON CONFLICT (instance_id, category, key) \
         DO UPDATE SET value = {}, updated_at = now()",
        sql_text(category),
        sql_text(key),
        sql_text(value),
        sql_text(value),
    ))
    .expect("set_preference SPI failed");
    "ok"
}

/// Delete a preference for the self instance.
#[pg_extern]
fn delete_preference(category: &str, key: &str) -> &'static str {
    let deleted = Spi::get_one::<bool>(&format!(
        "WITH d AS (\
             DELETE FROM kerai.preferences \
             WHERE instance_id = (SELECT id FROM kerai.instances WHERE is_self = true) \
             AND category = {} AND key = {} \
             RETURNING id\
         ) SELECT EXISTS(SELECT 1 FROM d)",
        sql_text(category),
        sql_text(key),
    ))
    .unwrap_or(Some(false))
    .unwrap_or(false);

    if deleted {
        "deleted"
    } else {
        "not_found"
    }
}

/// List all preferences for the self instance in a category.
#[pg_extern]
fn list_preferences(
    category: &str,
) -> TableIterator<'static, (name!(key, String), name!(value, String), name!(updated_at, String))>
{
    let query = format!(
        "SELECT key, value, updated_at::text \
         FROM kerai.preferences \
         WHERE instance_id = (SELECT id FROM kerai.instances WHERE is_self = true) \
         AND category = {} \
         ORDER BY key",
        sql_text(category),
    );

    let mut rows = Vec::new();
    Spi::connect(|client| {
        let tup_table = client.select(&query, None, None).unwrap();
        for row in tup_table {
            let key: String = row.get_by_name("key").unwrap().unwrap_or_default();
            let value: String = row.get_by_name("value").unwrap().unwrap_or_default();
            let updated_at: String = row.get_by_name("updated_at").unwrap().unwrap_or_default();
            rows.push((key, value, updated_at));
        }
    });

    TableIterator::new(rows)
}

/// Export all preferences in a category as JSON (for cache sync).
#[pg_extern]
fn export_preferences(category: &str) -> String {
    let query = format!(
        "SELECT json_agg(json_build_object('key', key, 'value', value)) \
         FROM kerai.preferences \
         WHERE instance_id = (SELECT id FROM kerai.instances WHERE is_self = true) \
         AND category = {}",
        sql_text(category),
    );

    Spi::get_one::<String>(&query)
        .unwrap_or(None)
        .unwrap_or_else(|| "[]".to_string())
}
