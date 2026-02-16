use postgres::Client;

use crate::output::{print_json, print_rows, OutputFormat};

pub fn launch(
    client: &mut Client,
    task_id: &str,
    agents: i32,
    kind: &str,
    model: Option<&str>,
    format: &OutputFormat,
) -> Result<(), String> {
    let row = client
        .query_one(
            "SELECT kerai.launch_swarm($1::uuid, $2, $3, $4)::text",
            &[&task_id, &agents, &kind, &model],
        )
        .map_err(|e| format!("launch_swarm failed: {e}"))?;

    let text: String = row.get(0);
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Invalid JSON: {e}"))?;

    let swarm_name = value["swarm_name"].as_str().unwrap_or("unknown");
    println!("Launched swarm '{swarm_name}' with {agents} agents");

    print_json(&value, format);
    Ok(())
}

pub fn status(
    client: &mut Client,
    task_id: Option<&str>,
    format: &OutputFormat,
) -> Result<(), String> {
    let row = client
        .query_one(
            "SELECT kerai.swarm_status($1::uuid)::text",
            &[&task_id],
        )
        .map_err(|e| format!("swarm_status failed: {e}"))?;

    let text: String = row.get(0);
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Invalid JSON: {e}"))?;

    let arr = value.as_array().ok_or("Expected JSON array")?;

    if arr.is_empty() {
        println!("No tasks found.");
        return Ok(());
    }

    let columns = vec![
        "task_id".into(),
        "description".into(),
        "status".into(),
        "swarm_name".into(),
        "total".into(),
        "passed".into(),
        "failed".into(),
    ];

    let rows: Vec<Vec<String>> = arr
        .iter()
        .map(|t| {
            vec![
                t["task_id"].as_str().unwrap_or("").chars().take(8).collect::<String>(),
                t["description"].as_str().unwrap_or("").to_string(),
                t["status"].as_str().unwrap_or("").to_string(),
                t["swarm_name"].as_str().unwrap_or("").to_string(),
                t["total_results"].as_i64().map(|n| n.to_string()).unwrap_or("0".into()),
                t["passed"].as_i64().map(|n| n.to_string()).unwrap_or("0".into()),
                t["failed"].as_i64().map(|n| n.to_string()).unwrap_or("0".into()),
            ]
        })
        .collect();

    print_rows(&columns, &rows, format);
    Ok(())
}

pub fn stop(client: &mut Client, task_id: &str) -> Result<(), String> {
    let row = client
        .query_one(
            "SELECT kerai.stop_swarm($1::uuid)::text",
            &[&task_id],
        )
        .map_err(|e| format!("stop_swarm failed: {e}"))?;

    let text: String = row.get(0);
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Invalid JSON: {e}"))?;

    let status = value["status"].as_str().unwrap_or("unknown");
    println!("Task {task_id}: {status}");
    Ok(())
}

pub fn leaderboard(
    client: &mut Client,
    task_id: &str,
    format: &OutputFormat,
) -> Result<(), String> {
    let row = client
        .query_one(
            "SELECT kerai.swarm_leaderboard($1::uuid)::text",
            &[&task_id],
        )
        .map_err(|e| format!("swarm_leaderboard failed: {e}"))?;

    let text: String = row.get(0);
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Invalid JSON: {e}"))?;

    let arr = value.as_array().ok_or("Expected JSON array")?;

    if arr.is_empty() {
        println!("No results yet.");
        return Ok(());
    }

    let columns = vec![
        "agent".into(),
        "pass".into(),
        "fail".into(),
        "total".into(),
        "rate%".into(),
        "avg_ms".into(),
    ];

    let rows: Vec<Vec<String>> = arr
        .iter()
        .map(|a| {
            vec![
                a["agent_name"].as_str().unwrap_or("").to_string(),
                a["pass_count"].as_i64().map(|n| n.to_string()).unwrap_or("0".into()),
                a["fail_count"].as_i64().map(|n| n.to_string()).unwrap_or("0".into()),
                a["total"].as_i64().map(|n| n.to_string()).unwrap_or("0".into()),
                format!("{:.1}", a["pass_rate"].as_f64().unwrap_or(0.0)),
                a["avg_duration_ms"].as_f64().map(|n| format!("{:.0}", n)).unwrap_or("-".into()),
            ]
        })
        .collect();

    print_rows(&columns, &rows, format);
    Ok(())
}

pub fn progress(
    client: &mut Client,
    task_id: &str,
    format: &OutputFormat,
) -> Result<(), String> {
    let row = client
        .query_one(
            "SELECT kerai.swarm_progress($1::uuid)::text",
            &[&task_id],
        )
        .map_err(|e| format!("swarm_progress failed: {e}"))?;

    let text: String = row.get(0);
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Invalid JSON: {e}"))?;

    let arr = value.as_array().ok_or("Expected JSON array")?;

    if arr.is_empty() {
        println!("No results yet.");
        return Ok(());
    }

    let columns = vec![
        "bucket".into(),
        "total".into(),
        "passed".into(),
        "failed".into(),
        "rate%".into(),
    ];

    let rows: Vec<Vec<String>> = arr
        .iter()
        .map(|b| {
            vec![
                b["bucket"].as_str().unwrap_or("").to_string(),
                b["total"].as_i64().map(|n| n.to_string()).unwrap_or("0".into()),
                b["passed"].as_i64().map(|n| n.to_string()).unwrap_or("0".into()),
                b["failed"].as_i64().map(|n| n.to_string()).unwrap_or("0".into()),
                format!("{:.1}", b["pass_rate"].as_f64().unwrap_or(0.0)),
            ]
        })
        .collect();

    print_rows(&columns, &rows, format);
    Ok(())
}
