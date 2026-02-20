use pgrx::prelude::*;

use crate::sql::sql_text;

const SELF_INSTANCE: &str =
    "(SELECT id FROM kerai.instances WHERE is_self = true)";

/// Render all preferences as a kerai language document and push onto stack.
#[pg_extern]
fn pull_init() -> &'static str {
    let mut doc = String::new();

    // Aliases section
    let mut aliases = Vec::new();
    Spi::connect(|client| {
        let tup_table = client
            .select(
                &format!(
                    "SELECT key, value FROM kerai.preferences \
                     WHERE instance_id = {SELF_INSTANCE} AND category = 'alias' \
                     ORDER BY key"
                ),
                None,
                &[],
            )
            .unwrap();
        for row in tup_table {
            let key: String = row.get_by_name("key").unwrap().unwrap_or_default();
            let value: String = row.get_by_name("value").unwrap().unwrap_or_default();
            aliases.push((key, value));
        }
    });

    if !aliases.is_empty() {
        doc.push_str("# aliases\n");
        for (key, value) in &aliases {
            doc.push_str(&format!("{key}: {value}\n"));
        }
    }

    // Config section
    let mut configs = Vec::new();
    Spi::connect(|client| {
        let tup_table = client
            .select(
                &format!(
                    "SELECT key, value FROM kerai.preferences \
                     WHERE instance_id = {SELF_INSTANCE} AND category = 'config' \
                     ORDER BY key"
                ),
                None,
                &[],
            )
            .unwrap();
        for row in tup_table {
            let key: String = row.get_by_name("key").unwrap().unwrap_or_default();
            let value: String = row.get_by_name("value").unwrap().unwrap_or_default();
            configs.push((key, value));
        }
    });

    if !configs.is_empty() {
        if !doc.is_empty() {
            doc.push('\n');
        }
        doc.push_str("# config\n");
        for (key, value) in &configs {
            doc.push_str(&format!("{key} {value}\n"));
        }
    }

    if doc.is_empty() {
        doc.push_str("# empty — add aliases (name: target) and config (key value)\n");
    }

    // Push onto stack
    Spi::run(&format!(
        "SELECT kerai.stack_push({}, 'init')",
        sql_text(&doc),
    ))
    .expect("pull_init: stack_push failed");

    "ok"
}

/// Parse the stack top as a kerai init document and apply changes to preferences.
/// Returns a JSON summary of changes applied.
#[pg_extern]
fn push_init() -> String {
    let content = match Spi::get_one::<String>(&format!(
        "SELECT content FROM kerai.stack \
         WHERE instance_id = {SELF_INSTANCE} \
         ORDER BY position DESC LIMIT 1"
    ))
    .unwrap_or(None)
    {
        Some(c) => c,
        None => return r#"{"error":"stack is empty"}"#.to_string(),
    };

    let parsed = parse_init_doc(&content);

    // Load current preferences
    let current_aliases = load_prefs("alias");
    let current_configs = load_prefs("config");

    let mut added = 0i64;
    let mut updated = 0i64;
    let mut deleted = 0i64;

    // Apply aliases: upsert parsed, delete missing
    for (key, value) in &parsed.aliases {
        match current_aliases.iter().find(|(k, _)| k == key) {
            Some((_, v)) if v == value => {} // unchanged
            Some(_) => {
                upsert_pref("alias", key, value);
                updated += 1;
            }
            None => {
                upsert_pref("alias", key, value);
                added += 1;
            }
        }
    }
    for (key, _) in &current_aliases {
        if !parsed.aliases.iter().any(|(k, _)| k == key) {
            delete_pref("alias", key);
            deleted += 1;
        }
    }

    // Apply configs: upsert parsed, delete missing
    // Skip postgres.global.connection (bootstrap-only)
    for (key, value) in &parsed.configs {
        if key == "postgres.global.connection" {
            continue;
        }
        match current_configs.iter().find(|(k, _)| k == key) {
            Some((_, v)) if v == value => {} // unchanged
            Some(_) => {
                upsert_pref("config", key, value);
                updated += 1;
            }
            None => {
                upsert_pref("config", key, value);
                added += 1;
            }
        }
    }
    for (key, _) in &current_configs {
        if key == "postgres.global.connection" {
            continue;
        }
        if !parsed.configs.iter().any(|(k, _)| k == key) {
            delete_pref("config", key);
            deleted += 1;
        }
    }

    format!(r#"{{"added":{added},"updated":{updated},"deleted":{deleted}}}"#)
}

/// Show what push_init would change without applying.
/// Returns a JSON array of change objects.
#[pg_extern]
fn diff_init() -> String {
    let content = match Spi::get_one::<String>(&format!(
        "SELECT content FROM kerai.stack \
         WHERE instance_id = {SELF_INSTANCE} \
         ORDER BY position DESC LIMIT 1"
    ))
    .unwrap_or(None)
    {
        Some(c) => c,
        None => return r#"[]"#.to_string(),
    };

    let parsed = parse_init_doc(&content);
    let current_aliases = load_prefs("alias");
    let current_configs = load_prefs("config");

    let mut changes = Vec::new();

    // Diff aliases
    for (key, value) in &parsed.aliases {
        match current_aliases.iter().find(|(k, _)| k == key) {
            Some((_, v)) if v == value => {}
            Some((_, old)) => {
                changes.push(format!(
                    r#"{{"op":"update","category":"alias","key":"{}","old":"{}","new":"{}"}}"#,
                    json_escape(key),
                    json_escape(old),
                    json_escape(value),
                ));
            }
            None => {
                changes.push(format!(
                    r#"{{"op":"add","category":"alias","key":"{}","value":"{}"}}"#,
                    json_escape(key),
                    json_escape(value),
                ));
            }
        }
    }
    for (key, value) in &current_aliases {
        if !parsed.aliases.iter().any(|(k, _)| k == key) {
            changes.push(format!(
                r#"{{"op":"delete","category":"alias","key":"{}","value":"{}"}}"#,
                json_escape(key),
                json_escape(value),
            ));
        }
    }

    // Diff configs (skip connection string)
    for (key, value) in &parsed.configs {
        if key == "postgres.global.connection" {
            continue;
        }
        match current_configs.iter().find(|(k, _)| k == key) {
            Some((_, v)) if v == value => {}
            Some((_, old)) => {
                changes.push(format!(
                    r#"{{"op":"update","category":"config","key":"{}","old":"{}","new":"{}"}}"#,
                    json_escape(key),
                    json_escape(old),
                    json_escape(value),
                ));
            }
            None => {
                changes.push(format!(
                    r#"{{"op":"add","category":"config","key":"{}","value":"{}"}}"#,
                    json_escape(key),
                    json_escape(value),
                ));
            }
        }
    }
    for (key, value) in &current_configs {
        if key == "postgres.global.connection" {
            continue;
        }
        if !parsed.configs.iter().any(|(k, _)| k == key) {
            changes.push(format!(
                r#"{{"op":"delete","category":"config","key":"{}","value":"{}"}}"#,
                json_escape(key),
                json_escape(value),
            ));
        }
    }

    format!("[{}]", changes.join(","))
}

// --- Internal helpers ---

struct InitDoc {
    aliases: Vec<(String, String)>,
    configs: Vec<(String, String)>,
}

/// Parse a kerai init document into aliases and config entries.
/// Lines with `:` (not at start) → alias definition.
/// Lines with space but no `:` → config call.
/// Comments (#, //) and blank lines are skipped.
fn parse_init_doc(content: &str) -> InitDoc {
    let mut aliases = Vec::new();
    let mut configs = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.starts_with("//")
        {
            continue;
        }

        // Definition: `name: target` (colon not at start)
        if let Some(colon_pos) = trimmed.find(':') {
            if colon_pos > 0 {
                let key = trimmed[..colon_pos].trim();
                let value = trimmed[colon_pos + 1..].trim();
                if !key.is_empty() && !value.is_empty() {
                    aliases.push((key.to_string(), value.to_string()));
                    continue;
                }
            }
        }

        // Call: `key value` (first word is key, rest is value)
        if let Some(space_pos) = trimmed.find(char::is_whitespace) {
            let key = trimmed[..space_pos].trim();
            let value = trimmed[space_pos..].trim();
            if !key.is_empty() && !value.is_empty() {
                configs.push((key.to_string(), value.to_string()));
            }
        }
    }

    InitDoc { aliases, configs }
}

fn load_prefs(category: &str) -> Vec<(String, String)> {
    let mut prefs = Vec::new();
    Spi::connect(|client| {
        let tup_table = client
            .select(
                &format!(
                    "SELECT key, value FROM kerai.preferences \
                     WHERE instance_id = {SELF_INSTANCE} AND category = {} \
                     ORDER BY key",
                    sql_text(category),
                ),
                None,
                &[],
            )
            .unwrap();
        for row in tup_table {
            let key: String = row.get_by_name("key").unwrap().unwrap_or_default();
            let value: String = row.get_by_name("value").unwrap().unwrap_or_default();
            prefs.push((key, value));
        }
    });
    prefs
}

fn upsert_pref(category: &str, key: &str, value: &str) {
    Spi::run(&format!(
        "SELECT kerai.set_preference({}, {}, {})",
        sql_text(category),
        sql_text(key),
        sql_text(value),
    ))
    .expect("upsert_pref failed");
}

fn delete_pref(category: &str, key: &str) {
    Spi::run(&format!(
        "SELECT kerai.delete_preference({}, {})",
        sql_text(category),
        sql_text(key),
    ))
    .expect("delete_pref failed");
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}
