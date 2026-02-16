use postgres::Client;

use crate::output::{print_rows, OutputFormat};

pub fn run(
    client: &mut Client,
    pattern: &str,
    kind: Option<&str>,
    limit: Option<i32>,
    format: &OutputFormat,
) -> Result<(), String> {
    let row = client
        .query_one(
            "SELECT kerai.find($1, $2, $3)::text",
            &[&pattern, &kind, &limit],
        )
        .map_err(|e| format!("find failed: {e}"))?;

    let text: String = row.get(0);
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Invalid JSON: {e}"))?;

    let arr = value.as_array().ok_or("Expected JSON array")?;

    if arr.is_empty() {
        println!("No matches found.");
        return Ok(());
    }

    let columns = vec![
        "kind".into(),
        "content".into(),
        "path".into(),
        "id".into(),
    ];

    let rows: Vec<Vec<String>> = arr
        .iter()
        .map(|n| {
            vec![
                n["kind"].as_str().unwrap_or("").to_string(),
                n["content"].as_str().unwrap_or("").to_string(),
                n["path"].as_str().unwrap_or("").to_string(),
                n["id"].as_str().unwrap_or("").to_string(),
            ]
        })
        .collect();

    println!("{} match(es)", rows.len());
    print_rows(&columns, &rows, format);
    Ok(())
}
