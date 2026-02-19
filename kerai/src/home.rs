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
