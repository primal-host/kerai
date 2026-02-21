/// Pass 0 — Registry: persistent infrastructure tables for CSV projects.
use pgrx::prelude::*;
use crate::sql::{sql_escape, sql_text};

/// Ensure the registry tables exist (idempotent).
pub fn ensure_registry_tables() {
    Spi::run(
        "CREATE TABLE IF NOT EXISTS kerai.csv_projects (
            id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            name        TEXT NOT NULL UNIQUE,
            schema_name TEXT NOT NULL,
            source_dir  TEXT,
            created_at  TIMESTAMPTZ DEFAULT now()
        )"
    ).expect("Failed to create csv_projects table");

    Spi::run(
        "CREATE TABLE IF NOT EXISTS kerai.csv_files (
            id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            project_id  UUID NOT NULL REFERENCES kerai.csv_projects(id),
            filename    TEXT NOT NULL,
            table_name  TEXT NOT NULL,
            headers     TEXT[] NOT NULL,
            row_count   INTEGER,
            created_at  TIMESTAMPTZ DEFAULT now(),
            UNIQUE (project_id, filename)
        )"
    ).expect("Failed to create csv_files table");
}

/// Register or get a project, returning its UUID.
pub fn register_project(name: &str, schema_name: &str, source_dir: Option<&str>) -> String {
    let src = match source_dir {
        Some(d) => sql_text(d),
        None => "NULL".to_string(),
    };

    Spi::get_one::<String>(&format!(
        "INSERT INTO kerai.csv_projects (name, schema_name, source_dir)
         VALUES ({}, {}, {})
         ON CONFLICT (name) DO UPDATE SET source_dir = EXCLUDED.source_dir
         RETURNING id::text",
        sql_text(name),
        sql_text(schema_name),
        src,
    ))
    .expect("Failed to register project")
    .expect("No project ID returned")
}

/// Register a file in the project, returning its UUID.
pub fn register_file(
    project_id: &str,
    filename: &str,
    table_name: &str,
    headers: &[String],
) -> String {
    // Build a Postgres text array literal
    let headers_arr = format!(
        "ARRAY[{}]::text[]",
        headers
            .iter()
            .map(|h| sql_text(h))
            .collect::<Vec<_>>()
            .join(", ")
    );

    Spi::get_one::<String>(&format!(
        "INSERT INTO kerai.csv_files (project_id, filename, table_name, headers)
         VALUES ('{}'::uuid, {}, {}, {})
         ON CONFLICT (project_id, filename) DO UPDATE
           SET table_name = EXCLUDED.table_name,
               headers = EXCLUDED.headers
         RETURNING id::text",
        sql_escape(project_id),
        sql_text(filename),
        sql_text(table_name),
        headers_arr,
    ))
    .expect("Failed to register file")
    .expect("No file ID returned")
}

/// Update the row count for a registered file.
pub fn update_row_count(file_id: &str, row_count: i64) {
    Spi::run(&format!(
        "UPDATE kerai.csv_files SET row_count = {} WHERE id = '{}'::uuid",
        row_count,
        sql_escape(file_id),
    ))
    .expect("Failed to update row_count");
}
