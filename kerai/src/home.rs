use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_ALIASES: &str = "\
# common aliases for kerai libraries
:pg postgres
";

/// Creates `~/.kerai/` if it doesn't exist, returns its path.
pub fn ensure_home_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("could not determine home directory")?;
    let kerai_home = home.join(".kerai");
    if !kerai_home.exists() {
        fs::create_dir_all(&kerai_home)
            .map_err(|e| format!("failed to create ~/.kerai: {e}"))?;
    }
    Ok(kerai_home)
}

/// Creates `aliases.kerai` with default content if missing, returns its path.
pub fn ensure_aliases_file(home: &Path) -> Result<PathBuf, String> {
    let path = home.join("aliases.kerai");
    if !path.exists() {
        fs::write(&path, DEFAULT_ALIASES)
            .map_err(|e| format!("failed to create aliases.kerai: {e}"))?;
    }
    Ok(path)
}

/// Parses `aliases.kerai` into a map of alias → target namespace.
///
/// Format:
/// - Skip empty lines
/// - Skip lines where trimmed content starts with `#` or `//`
/// - Lines starting with `:` (after trim): split into alias name + target
pub fn load_aliases(home: &Path) -> Result<HashMap<String, String>, String> {
    let path = home.join("aliases.kerai");
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let content = fs::read_to_string(&path)
        .map_err(|e| format!("failed to read aliases.kerai: {e}"))?;

    let mut aliases = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix(':') {
            let mut parts = rest.splitn(2, char::is_whitespace);
            if let (Some(alias), Some(target)) = (parts.next(), parts.next()) {
                let alias = alias.trim();
                let target = target.trim();
                if !alias.is_empty() && !target.is_empty() {
                    aliases.insert(alias.to_string(), target.to_string());
                }
            }
        }
    }
    Ok(aliases)
}

const KERAI_FILE_HEADER: &str = "\
# kerai-controlled configuration — do not hand-edit
# syntax: :name target (definition) | name arg (function call) | name: type (reserved)
";

/// Creates `kerai.kerai` with header comment if missing, returns its path.
pub fn ensure_kerai_file(home: &Path) -> Result<PathBuf, String> {
    let path = home.join("kerai.kerai");
    if !path.exists() {
        fs::write(&path, KERAI_FILE_HEADER)
            .map_err(|e| format!("failed to create kerai.kerai: {e}"))?;
    }
    Ok(path)
}

/// Parses `kerai.kerai` function-call lines into a key→value map.
///
/// Only lines matching the "name arg" pattern are parsed (no leading colon,
/// no trailing colon on first token). Comments and definition lines are skipped.
pub fn load_kerai_file(home: &Path) -> Result<HashMap<String, String>, String> {
    let path = home.join("kerai.kerai");
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let content = fs::read_to_string(&path)
        .map_err(|e| format!("failed to read kerai.kerai: {e}"))?;

    let mut map = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }
        // Skip definition lines (:name target)
        if trimmed.starts_with(':') {
            continue;
        }
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
            // Skip type-annotation lines (name: type)
            if key.ends_with(':') {
                continue;
            }
            let value = value.trim();
            if !key.is_empty() && !value.is_empty() {
                map.insert(key.to_string(), value.to_string());
            }
        }
    }
    Ok(map)
}

/// Sets a key-value pair in `kerai.kerai`, replacing an existing line or appending.
pub fn set_kerai_value(home: &Path, key: &str, value: &str) -> Result<(), String> {
    let path = home.join("kerai.kerai");
    let content = if path.exists() {
        fs::read_to_string(&path)
            .map_err(|e| format!("failed to read kerai.kerai: {e}"))?
    } else {
        KERAI_FILE_HEADER.to_string()
    };

    let mut found = false;
    let mut lines: Vec<String> = content
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if !trimmed.is_empty()
                && !trimmed.starts_with('#')
                && !trimmed.starts_with("//")
                && !trimmed.starts_with(':')
            {
                if let Some((k, _)) = trimmed.split_once(char::is_whitespace) {
                    if !k.ends_with(':') && k == key {
                        found = true;
                        return format!("{key} {value}");
                    }
                }
            }
            line.to_string()
        })
        .collect();

    if !found {
        lines.push(format!("{key} {value}"));
    }

    let mut output = lines.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }

    fs::write(&path, output)
        .map_err(|e| format!("failed to write kerai.kerai: {e}"))?;
    Ok(())
}
