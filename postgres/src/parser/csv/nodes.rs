/// Pass 3 — Kerai Nodes + Edges: create structural knowledge graph from CSV metadata.
use serde_json::json;
use uuid::Uuid;

use crate::parser::ast_walker::{EdgeRow, NodeRow};
use crate::parser::inserter;
use crate::parser::path_builder::PathContext;
use super::kinds;
use super::promote::ColumnStats;

/// File metadata for node creation.
pub struct FileInfo {
    pub filename: String,
    pub table_name: String,
    pub schema: String,
    pub row_count: i64,
    pub column_stats: Vec<ColumnStats>,
}

/// Create the dataset node (root) for a project.
/// Returns the dataset node ID.
pub fn create_dataset_node(
    instance_id: &str,
    project_name: &str,
    schema: &str,
    source_dir: Option<&str>,
    files: &[FileInfo],
) -> String {
    let dataset_id = Uuid::new_v4().to_string();
    let path_ctx = PathContext::with_root(project_name);

    let total_rows: i64 = files.iter().map(|f| f.row_count).sum();
    let total_cols: usize = files.iter().map(|f| f.column_stats.len()).sum();

    let mut metadata = json!({
        "schema": schema,
        "project": project_name,
        "table_count": files.len(),
        "total_rows": total_rows,
        "total_columns": total_cols,
    });
    if let Some(dir) = source_dir {
        metadata["source_dir"] = json!(dir);
    }

    let node = NodeRow {
        id: dataset_id.clone(),
        instance_id: instance_id.to_string(),
        kind: kinds::CSV_DATASET.to_string(),
        language: Some("csv".to_string()),
        content: Some(project_name.to_string()),
        parent_id: None,
        position: 0,
        path: path_ctx.path(),
        metadata,
        span_start: None,
        span_end: None,
    };
    inserter::insert_nodes(&[node]);

    dataset_id
}

/// Create table and column nodes for all files, plus shared_column edges.
pub fn create_table_and_column_nodes(
    instance_id: &str,
    dataset_id: &str,
    project_name: &str,
    project_id: &str,
    files: &[FileInfo],
) -> (usize, usize) {
    let mut all_nodes: Vec<NodeRow> = Vec::new();
    let mut all_edges: Vec<EdgeRow> = Vec::new();

    // Track column_name → (table_name, column_node_id) for shared_column detection
    let mut column_registry: std::collections::HashMap<String, Vec<(String, String)>> =
        std::collections::HashMap::new();

    for (file_idx, file) in files.iter().enumerate() {
        let mut path_ctx = PathContext::with_root(project_name);
        path_ctx.push(&file.table_name);

        let table_node_id = Uuid::new_v4().to_string();
        let qualified_name = format!("{}.{}", file.schema, file.table_name);

        let nil_total: i64 = file.column_stats.iter().map(|c| c.nil_count).sum();

        let table_metadata = json!({
            "schema": file.schema,
            "table_name": file.table_name,
            "source_file": file.filename,
            "row_count": file.row_count,
            "column_count": file.column_stats.len(),
            "nil_total": nil_total,
            "qualified_name": qualified_name,
            "project_id": project_id,
        });

        all_nodes.push(NodeRow {
            id: table_node_id.clone(),
            instance_id: instance_id.to_string(),
            kind: kinds::CSV_TABLE.to_string(),
            language: Some("csv".to_string()),
            content: Some(file.table_name.clone()),
            parent_id: Some(dataset_id.to_string()),
            position: file_idx as i32,
            path: path_ctx.path(),
            metadata: table_metadata,
            span_start: None,
            span_end: None,
        });

        // Create column nodes
        for col_stat in &file.column_stats {
            let col_node_id = Uuid::new_v4().to_string();
            let col_path = path_ctx.child_path(&col_stat.name);

            all_nodes.push(NodeRow {
                id: col_node_id.clone(),
                instance_id: instance_id.to_string(),
                kind: kinds::CSV_COLUMN.to_string(),
                language: Some("csv".to_string()),
                content: Some(col_stat.name.clone()),
                parent_id: Some(table_node_id.clone()),
                position: col_stat.position,
                path: Some(col_path),
                metadata: col_stat.to_json(),
                span_start: None,
                span_end: None,
            });

            // Register for shared_column detection
            column_registry
                .entry(col_stat.name.clone())
                .or_default()
                .push((file.table_name.clone(), col_node_id));
        }
    }

    // Create shared_column edges between columns with the same name in different tables
    for (col_name, locations) in &column_registry {
        if locations.len() < 2 {
            continue;
        }
        // Create edges between all pairs (i, j) where i < j
        for i in 0..locations.len() {
            for j in (i + 1)..locations.len() {
                let (ref source_table, ref source_id) = locations[i];
                let (ref target_table, ref target_id) = locations[j];

                all_edges.push(EdgeRow {
                    id: Uuid::new_v4().to_string(),
                    source_id: source_id.clone(),
                    target_id: target_id.clone(),
                    relation: "shared_column".to_string(),
                    metadata: json!({
                        "column_name": col_name,
                        "source_table": source_table,
                        "target_table": target_table,
                    }),
                });
            }
        }
    }

    let node_count = all_nodes.len();
    let edge_count = all_edges.len();

    inserter::insert_nodes(&all_nodes);
    inserter::insert_edges(&all_edges);

    (node_count, edge_count)
}
