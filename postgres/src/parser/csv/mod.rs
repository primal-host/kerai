/// CSV parser module — CSV files → typed Postgres tables + kerai.nodes + kerai.edges.
///
/// Multi-pass architecture:
/// - Pass 0: Registry — create persistent metadata tables
/// - Pass 1: Raw Ingest — create TEXT tables, batch INSERT all data
/// - Pass 2: Type Promotion — analyze and promote columns to typed
/// - Pass 3: Kerai Nodes — create structural knowledge graph
use pgrx::prelude::*;
use serde_json::json;
use std::path::Path;
use std::time::Instant;

pub mod kinds;
mod registry;
pub mod ingest;
mod promote;
mod nodes;

use crate::sql::sql_escape;

/// Delete all CSV-related kerai nodes for a project (idempotent cleanup).
fn delete_csv_nodes(instance_id: &str, project_name: &str) {
    // Find the dataset node
    let dataset_id = Spi::get_one::<String>(&format!(
        "SELECT id::text FROM kerai.nodes
         WHERE instance_id = '{}'::uuid
         AND kind = '{}' AND content = '{}'",
        sql_escape(instance_id),
        kinds::CSV_DATASET,
        sql_escape(project_name),
    ))
    .unwrap_or(None);

    if let Some(did) = dataset_id {
        // Delete edges involving any descendant
        Spi::run(&format!(
            "WITH RECURSIVE descendants AS (
                SELECT id FROM kerai.nodes WHERE id = '{}'::uuid
                UNION ALL
                SELECT n.id FROM kerai.nodes n
                JOIN descendants d ON n.parent_id = d.id
            )
            DELETE FROM kerai.edges WHERE source_id IN (SELECT id FROM descendants)
                OR target_id IN (SELECT id FROM descendants)",
            sql_escape(&did),
        ))
        .ok();

        // Delete descendant nodes (children first via reverse traversal)
        Spi::run(&format!(
            "WITH RECURSIVE descendants AS (
                SELECT id FROM kerai.nodes WHERE id = '{}'::uuid
                UNION ALL
                SELECT n.id FROM kerai.nodes n
                JOIN descendants d ON n.parent_id = d.id
            )
            DELETE FROM kerai.nodes WHERE id IN (SELECT id FROM descendants)",
            sql_escape(&did),
        ))
        .ok();
    }
}

/// Parse a single CSV file: create typed table + kerai nodes.
///
/// Returns JSON: `{file, schema, table, rows, columns, nodes, edges, elapsed_ms}`.
#[pg_extern]
fn parse_csv_file(
    path: &str,
    schema_name: &str,
    project_name: &str,
) -> pgrx::JsonB {
    let start = Instant::now();
    let file_path = Path::new(path);

    if !file_path.exists() {
        pgrx::error!("CSV file does not exist: {}", path);
    }

    let instance_id = super::get_self_instance_id();

    // Pass 0: Registry
    registry::ensure_registry_tables();
    ensure_schema(schema_name);

    let project_id = registry::register_project(project_name, schema_name, None);

    // Read the file
    let content = std::fs::read_to_string(file_path)
        .unwrap_or_else(|e| pgrx::error!("Failed to read CSV file: {}", e));

    let filename = file_path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string());

    // Process single file through Pass 1 + Pass 2
    let result = process_single_file(&content, &filename, schema_name, &project_id);

    let (table_name_out, row_count, col_count, file_infos) = match result {
        Some((fname, tname, rows, col_stats)) => {
            let fi = nodes::FileInfo {
                filename: fname,
                table_name: tname.clone(),
                schema: schema_name.to_string(),
                row_count: rows,
                column_stats: col_stats,
            };
            (tname, rows, fi.column_stats.len(), vec![fi])
        }
        None => (String::new(), 0, 0, vec![]),
    };

    // Pass 3: Create nodes
    delete_csv_nodes(&instance_id, project_name);

    let dataset_id = nodes::create_dataset_node(
        &instance_id,
        project_name,
        schema_name,
        None,
        &file_infos,
    );

    let (node_count, edge_count) = if !file_infos.is_empty() {
        nodes::create_table_and_column_nodes(
            &instance_id,
            &dataset_id,
            project_name,
            &project_id,
            &file_infos,
        )
    } else {
        (0, 0)
    };

    let total_nodes = node_count + 1; // +1 for dataset node

    // Auto-mint reward
    mint_csv_reward(project_name, total_nodes, edge_count);

    let elapsed = start.elapsed();
    pgrx::JsonB(json!({
        "file": filename,
        "schema": schema_name,
        "table": table_name_out,
        "rows": row_count,
        "columns": col_count,
        "nodes": total_nodes,
        "edges": edge_count,
        "elapsed_ms": elapsed.as_millis() as u64,
    }))
}

/// Parse an entire directory of CSV files: create typed tables + kerai nodes.
///
/// Returns JSON: `{project, schema, files, total_rows, nodes, edges, elapsed_ms}`.
#[pg_extern]
fn parse_csv_dir(
    dir_path: &str,
    schema_name: &str,
    project_name: &str,
) -> pgrx::JsonB {
    let start = Instant::now();
    let dir = Path::new(dir_path);

    if !dir.exists() || !dir.is_dir() {
        pgrx::error!("Directory does not exist: {}", dir_path);
    }

    let instance_id = super::get_self_instance_id();

    // Pass 0: Registry
    registry::ensure_registry_tables();
    ensure_schema(schema_name);

    let project_id = registry::register_project(project_name, schema_name, Some(dir_path));

    // Discover CSV files
    let mut csv_files: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| pgrx::error!("Failed to read directory: {}", e))
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("csv") {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    csv_files.sort();

    if csv_files.is_empty() {
        return pgrx::JsonB(json!({
            "project": project_name,
            "schema": schema_name,
            "files": 0,
            "total_rows": 0,
            "nodes": 0,
            "edges": 0,
            "elapsed_ms": start.elapsed().as_millis() as u64,
        }));
    }

    // Process each file through Pass 1 + Pass 2
    let mut file_infos: Vec<nodes::FileInfo> = Vec::new();
    let mut file_results: Vec<serde_json::Value> = Vec::new();
    let mut total_rows: i64 = 0;

    for csv_path in &csv_files {
        let filename = csv_path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();

        let content = match std::fs::read_to_string(csv_path) {
            Ok(c) => c,
            Err(e) => {
                pgrx::warning!("Skipping {}: {}", filename, e);
                continue;
            }
        };

        if let Some((fname, tname, row_count, col_stats)) =
            process_single_file(&content, &filename, schema_name, &project_id)
        {
            file_results.push(json!({
                "file": fname,
                "table": tname,
                "rows": row_count,
                "columns": col_stats.len(),
            }));

            total_rows += row_count;

            file_infos.push(nodes::FileInfo {
                filename: fname,
                table_name: tname,
                schema: schema_name.to_string(),
                row_count,
                column_stats: col_stats,
            });
        }
    }

    // Pass 3: Create nodes (delete old ones first)
    delete_csv_nodes(&instance_id, project_name);

    let dataset_id = nodes::create_dataset_node(
        &instance_id,
        project_name,
        schema_name,
        Some(dir_path),
        &file_infos,
    );

    let (node_count, edge_count) = nodes::create_table_and_column_nodes(
        &instance_id,
        &dataset_id,
        project_name,
        &project_id,
        &file_infos,
    );

    let total_nodes = node_count + 1; // +1 for dataset node

    // Auto-mint reward
    mint_csv_reward(project_name, total_nodes, edge_count);

    let elapsed = start.elapsed();
    pgrx::JsonB(json!({
        "project": project_name,
        "schema": schema_name,
        "files": file_infos.len(),
        "total_rows": total_rows,
        "nodes": total_nodes,
        "edges": edge_count,
        "results": file_results,
        "elapsed_ms": elapsed.as_millis() as u64,
    }))
}

/// Process a single CSV file through Pass 1 (ingest) and Pass 2 (promote).
/// Returns (filename, table_name, row_count, column_stats) or None on failure.
fn process_single_file(
    content: &str,
    filename: &str,
    schema_name: &str,
    project_id: &str,
) -> Option<(String, String, i64, Vec<promote::ColumnStats>)> {
    // Parse headers
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(content.as_bytes());

    let headers: Vec<String> = match reader.headers() {
        Ok(h) => h.iter().map(|s| s.to_string()).collect(),
        Err(e) => {
            pgrx::warning!("Failed to read headers from {}: {}", filename, e);
            return None;
        }
    };

    if headers.is_empty() {
        pgrx::warning!("No headers found in {}", filename);
        return None;
    }

    let table_name = ingest::derive_table_name(filename);

    // Sanitize and deduplicate column names
    let sanitized: Vec<String> = headers.iter().map(|h| ingest::sanitize_column_name(h)).collect();
    let columns = ingest::deduplicate_columns(&sanitized);

    // Register file
    let file_id = registry::register_file(project_id, filename, &table_name, &headers);

    // Pass 1: Create raw TEXT table and load data
    let qualified = ingest::create_raw_table(schema_name, &table_name, &columns);
    let row_count = ingest::load_raw_data(&qualified, &columns, content);
    registry::update_row_count(&file_id, row_count);

    // Pass 2: Type promotion
    let col_stats = promote::promote_columns(&qualified, &columns, &headers);

    Some((filename.to_string(), table_name, row_count, col_stats))
}

/// Ensure the target schema exists.
fn ensure_schema(schema_name: &str) {
    Spi::run(&format!(
        "CREATE SCHEMA IF NOT EXISTS \"{}\"",
        sql_escape(schema_name),
    ))
    .expect("Failed to create schema");
}

/// Auto-mint reward for CSV parsing.
fn mint_csv_reward(project_name: &str, node_count: usize, edge_count: usize) {
    if node_count > 0 {
        let details = json!({
            "project": project_name,
            "nodes": node_count,
            "edges": edge_count,
        });
        let details_str = details.to_string().replace('\'', "''");
        let _ = Spi::get_one::<pgrx::JsonB>(&format!(
            "SELECT kerai.mint_reward('parse_csv', '{}'::jsonb)",
            details_str,
        ));
    }
}
