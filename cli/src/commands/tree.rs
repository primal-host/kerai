use postgres::Client;

use crate::output::{print_rows, OutputFormat};

pub fn run(
    client: &mut Client,
    path: Option<&str>,
    format: &OutputFormat,
) -> Result<(), String> {
    let row = client
        .query_one("SELECT kerai.tree($1)::text", &[&path])
        .map_err(|e| format!("tree failed: {e}"))?;

    let text: String = row.get(0);
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Invalid JSON: {e}"))?;

    let arr = value.as_array().ok_or("Expected JSON array")?;

    if arr.is_empty() {
        println!("No nodes found.");
        return Ok(());
    }

    let columns = vec![
        "kind".into(),
        "content".into(),
        "path".into(),
        "children".into(),
    ];

    let rows: Vec<Vec<String>> = arr
        .iter()
        .map(|n| {
            vec![
                n["kind"].as_str().unwrap_or("").to_string(),
                n["content"].as_str().unwrap_or("").to_string(),
                n["path"].as_str().unwrap_or("").to_string(),
                n["child_count"].as_i64().unwrap_or(0).to_string(),
            ]
        })
        .collect();

    print_rows(&columns, &rows, format);
    Ok(())
}
