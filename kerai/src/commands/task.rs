use postgres::Client;

use crate::output::{print_json, print_rows, OutputFormat};

pub fn create(
    client: &mut Client,
    description: &str,
    success_command: &str,
    scope: Option<&str>,
    budget_ops: Option<i32>,
    budget_seconds: Option<i32>,
    format: &OutputFormat,
) -> Result<(), String> {
    let row = client
        .query_one(
            "SELECT kerai.create_task($1, $2, $3::uuid, $4, $5)::text",
            &[&description, &success_command, &scope, &budget_ops, &budget_seconds],
        )
        .map_err(|e| format!("create_task failed: {e}"))?;

    let text: String = row.get(0);
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Invalid JSON: {e}"))?;

    let id = value["id"].as_str().unwrap_or("unknown");
    println!("Created task {id} (status: pending)");

    print_json(&value, format);
    Ok(())
}

pub fn list(
    client: &mut Client,
    status: Option<&str>,
    format: &OutputFormat,
) -> Result<(), String> {
    let row = client
        .query_one("SELECT kerai.list_tasks($1)::text", &[&status])
        .map_err(|e| format!("list_tasks failed: {e}"))?;

    let text: String = row.get(0);
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Invalid JSON: {e}"))?;

    let arr = value.as_array().ok_or("Expected JSON array")?;

    if arr.is_empty() {
        println!("No tasks found.");
        return Ok(());
    }

    let columns = vec![
        "id".into(),
        "description".into(),
        "status".into(),
        "agent_kind".into(),
        "agent_count".into(),
        "swarm_name".into(),
        "created_at".into(),
    ];

    let rows: Vec<Vec<String>> = arr
        .iter()
        .map(|t| {
            vec![
                t["id"].as_str().unwrap_or("").chars().take(8).collect::<String>(),
                t["description"].as_str().unwrap_or("").to_string(),
                t["status"].as_str().unwrap_or("").to_string(),
                t["agent_kind"].as_str().unwrap_or("").to_string(),
                t["agent_count"].as_i64().map(|n| n.to_string()).unwrap_or_default(),
                t["swarm_name"].as_str().unwrap_or("").to_string(),
                t["created_at"].as_str().unwrap_or("").to_string(),
            ]
        })
        .collect();

    print_rows(&columns, &rows, format);
    Ok(())
}

pub fn show(
    client: &mut Client,
    task_id: &str,
    format: &OutputFormat,
) -> Result<(), String> {
    let row = client
        .query_one(
            "SELECT kerai.get_task($1::uuid)::text",
            &[&task_id],
        )
        .map_err(|e| format!("get_task failed: {e}"))?;

    let text: String = row.get(0);
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Invalid JSON: {e}"))?;

    print_json(&value, format);
    Ok(())
}
