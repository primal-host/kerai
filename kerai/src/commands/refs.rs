use postgres::Client;

use crate::output::{print_json, print_rows, OutputFormat};

pub fn run(
    client: &mut Client,
    symbol: &str,
    format: &OutputFormat,
) -> Result<(), String> {
    let row = client
        .query_one("SELECT kerai.refs($1)::text", &[&symbol])
        .map_err(|e| format!("refs failed: {e}"))?;

    let text: String = row.get(0);
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Invalid JSON: {e}"))?;

    match format {
        OutputFormat::Json => {
            print_json(&value, format);
        }
        _ => {
            println!("Symbol: {symbol}");
            println!();

            // Definitions
            if let Some(defs) = value["definitions"].as_array() {
                if !defs.is_empty() {
                    println!("Definitions ({}):", defs.len());
                    let columns = vec!["kind".into(), "content".into(), "path".into()];
                    let rows: Vec<Vec<String>> = defs
                        .iter()
                        .map(|d| {
                            vec![
                                d["kind"].as_str().unwrap_or("").to_string(),
                                d["content"].as_str().unwrap_or("").to_string(),
                                d["path"].as_str().unwrap_or("").to_string(),
                            ]
                        })
                        .collect();
                    print_rows(&columns, &rows, format);
                    println!();
                }
            }

            // Impl blocks
            if let Some(impls) = value["impls"].as_array() {
                if !impls.is_empty() {
                    println!("Impl blocks ({}):", impls.len());
                    let columns = vec!["kind".into(), "content".into(), "path".into()];
                    let rows: Vec<Vec<String>> = impls
                        .iter()
                        .map(|i| {
                            vec![
                                i["kind"].as_str().unwrap_or("").to_string(),
                                i["content"].as_str().unwrap_or("").to_string(),
                                i["path"].as_str().unwrap_or("").to_string(),
                            ]
                        })
                        .collect();
                    print_rows(&columns, &rows, format);
                    println!();
                }
            }

            // References
            if let Some(refs) = value["references"].as_array() {
                if !refs.is_empty() {
                    println!("References ({}):", refs.len());
                    let columns = vec![
                        "kind".into(),
                        "content".into(),
                        "parent_kind".into(),
                        "parent_content".into(),
                        "path".into(),
                    ];
                    let rows: Vec<Vec<String>> = refs
                        .iter()
                        .map(|r| {
                            vec![
                                r["kind"].as_str().unwrap_or("").to_string(),
                                r["content"].as_str().unwrap_or("").to_string(),
                                r["parent_kind"].as_str().unwrap_or("").to_string(),
                                r["parent_content"].as_str().unwrap_or("").to_string(),
                                r["path"].as_str().unwrap_or("").to_string(),
                            ]
                        })
                        .collect();
                    print_rows(&columns, &rows, format);
                }
            }

            // Summary if all empty
            let total = value["definitions"].as_array().map_or(0, |a| a.len())
                + value["impls"].as_array().map_or(0, |a| a.len())
                + value["references"].as_array().map_or(0, |a| a.len());
            if total == 0 {
                println!("No references found for '{symbol}'.");
            }
        }
    }

    Ok(())
}
