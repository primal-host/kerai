use postgres::Client;

use crate::output::{print_rows, OutputFormat};

pub fn show(client: &mut Client, format: &OutputFormat) -> Result<(), String> {
    let row = client
        .query_opt("SELECT kerai.stack_peek()", &[])
        .map_err(|e| format!("Failed to peek stack: {e}"))?;

    match row.and_then(|r| r.get::<_, Option<String>>(0)) {
        Some(content) => {
            match format {
                OutputFormat::Json => {
                    println!(
                        r#"{{"content":{}}}"#,
                        serde_json::to_string(&content).unwrap_or_else(|_| "null".to_string())
                    );
                }
                _ => println!("{content}"),
            }
        }
        None => {
            match format {
                OutputFormat::Json => println!(r#"{{"content":null}}"#),
                _ => println!("stack is empty"),
            }
        }
    }
    Ok(())
}

pub fn list(client: &mut Client, format: &OutputFormat) -> Result<(), String> {
    let rows = client
        .query(
            "SELECT position, label, preview, created_at FROM kerai.stack_list()",
            &[],
        )
        .map_err(|e| format!("Failed to list stack: {e}"))?;

    let columns = vec![
        "position".to_string(),
        "label".to_string(),
        "preview".to_string(),
        "createdAt".to_string(),
    ];
    let data: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            vec![
                r.get::<_, i32>(0).to_string(),
                r.get::<_, String>(1),
                r.get::<_, String>(2),
                r.get::<_, String>(3),
            ]
        })
        .collect();

    print_rows(&columns, &data, format);
    Ok(())
}

pub fn drop(client: &mut Client, format: &OutputFormat) -> Result<(), String> {
    let row = client
        .query_one("SELECT kerai.stack_drop()", &[])
        .map_err(|e| format!("Failed to drop stack: {e}"))?;

    let result: String = row.get(0);
    match format {
        OutputFormat::Json => println!(r#"{{"status":"{result}"}}"#),
        _ => println!("{result}"),
    }
    Ok(())
}

pub fn clear(client: &mut Client, format: &OutputFormat) -> Result<(), String> {
    let row = client
        .query_one("SELECT kerai.stack_clear()", &[])
        .map_err(|e| format!("Failed to clear stack: {e}"))?;

    let count: i32 = row.get(0);
    match format {
        OutputFormat::Json => println!(r#"{{"cleared":{count}}}"#),
        _ => println!("cleared {count} entries"),
    }
    Ok(())
}
