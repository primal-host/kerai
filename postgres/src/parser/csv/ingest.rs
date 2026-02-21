/// Pass 1 — Raw Ingest: create TEXT tables and load CSV data.
use pgrx::prelude::*;
use crate::sql::{sql_escape, sql_text};

const BATCH_SIZE: usize = 500;

/// Derive a Postgres table name from a CSV filename.
/// `MTeams.csv` → `m_teams`
/// `MNCAATourneyCompactResults.csv` → `mncaa_tourney_compact_results`
pub fn derive_table_name(filename: &str) -> String {
    let stem = filename.strip_suffix(".csv").unwrap_or(filename);
    camel_to_snake(stem)
}

/// Convert CamelCase to snake_case.
/// Handles runs of uppercase letters (acronyms) correctly:
/// `MNCAAFoo` → `m_ncaa_foo`
fn camel_to_snake(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 8);
    let chars: Vec<char> = s.chars().collect();

    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                let prev = chars[i - 1];
                let next = chars.get(i + 1);
                // Insert underscore before uppercase if:
                // - previous char is lowercase, OR
                // - previous char is uppercase AND next char is lowercase
                //   (end of an acronym like NCAA → ncaa before next word)
                if prev.is_lowercase()
                    || (prev.is_uppercase()
                        && next.is_some_and(|n| n.is_lowercase()))
                {
                    result.push('_');
                }
            }
            result.push(c.to_lowercase().next().unwrap_or(c));
        } else {
            result.push(c);
        }
    }

    result
}

/// Sanitize a CSV header into a valid Postgres column name.
/// Lowercases, replaces non-alphanumeric with underscore, prefix if starts with digit.
pub fn sanitize_column_name(header: &str) -> String {
    let mut name = String::with_capacity(header.len());
    for c in header.chars() {
        if c.is_alphanumeric() || c == '_' {
            name.push(c.to_lowercase().next().unwrap_or(c));
        } else {
            name.push('_');
        }
    }
    // Prefix with underscore if starts with digit
    if name.starts_with(|c: char| c.is_ascii_digit()) {
        name.insert(0, '_');
    }
    if name.is_empty() {
        name = "col".to_string();
    }
    name
}

/// Deduplicate column names by appending _2, _3 etc.
pub fn deduplicate_columns(columns: &[String]) -> Vec<String> {
    let mut result = Vec::with_capacity(columns.len());
    let mut counts = std::collections::HashMap::new();

    for col in columns {
        let count = counts.entry(col.clone()).or_insert(0u32);
        *count += 1;
        if *count == 1 {
            result.push(col.clone());
        } else {
            result.push(format!("{}_{}", col, count));
        }
    }
    result
}

/// Create a raw TEXT table in the given schema.
/// Returns the qualified table name: `schema.table_name`.
pub fn create_raw_table(schema: &str, table_name: &str, columns: &[String]) -> String {
    let qualified = format!("{}.{}", schema, table_name);

    // Drop if exists (idempotent)
    Spi::run(&format!("DROP TABLE IF EXISTS {} CASCADE", qualified))
        .expect("Failed to drop existing table");

    let col_defs: Vec<String> = columns
        .iter()
        .map(|c| format!("\"{}\" TEXT", sql_escape(c)))
        .collect();

    Spi::run(&format!(
        "CREATE TABLE {} ({})",
        qualified,
        col_defs.join(", "),
    ))
    .expect("Failed to create raw table");

    qualified
}

/// Load CSV data into a raw TEXT table using batch INSERTs.
/// Returns the number of rows inserted.
pub fn load_raw_data(
    qualified_table: &str,
    columns: &[String],
    csv_content: &str,
) -> i64 {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(csv_content.as_bytes());

    let col_list: String = columns
        .iter()
        .map(|c| format!("\"{}\"", sql_escape(c)))
        .collect::<Vec<_>>()
        .join(", ");

    let mut total_rows: i64 = 0;
    let mut batch_values: Vec<String> = Vec::with_capacity(BATCH_SIZE);

    for result in reader.records() {
        let record = match result {
            Ok(r) => r,
            Err(e) => {
                pgrx::warning!("Skipping malformed CSV row: {}", e);
                continue;
            }
        };

        let row_values: Vec<String> = (0..columns.len())
            .map(|i| {
                match record.get(i) {
                    Some(val) if !val.is_empty() => sql_text(val),
                    _ => "NULL".to_string(),
                }
            })
            .collect();

        batch_values.push(format!("({})", row_values.join(", ")));
        total_rows += 1;

        if batch_values.len() >= BATCH_SIZE {
            flush_batch(qualified_table, &col_list, &batch_values);
            batch_values.clear();
        }
    }

    // Flush remaining
    if !batch_values.is_empty() {
        flush_batch(qualified_table, &col_list, &batch_values);
    }

    total_rows
}

fn flush_batch(qualified_table: &str, col_list: &str, values: &[String]) {
    let sql = format!(
        "INSERT INTO {} ({}) VALUES {}",
        qualified_table,
        col_list,
        values.join(", "),
    );
    Spi::run(&sql).expect("Failed to insert batch");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_table_name() {
        assert_eq!(derive_table_name("MTeams.csv"), "m_teams");
        // MNCAA is treated as one uppercase run (can't distinguish M+NCAA without domain knowledge)
        assert_eq!(
            derive_table_name("MNCAATourneyCompactResults.csv"),
            "mncaa_tourney_compact_results"
        );
        assert_eq!(derive_table_name("Cities.csv"), "cities");
        assert_eq!(
            derive_table_name("MRegularSeasonCompactResults.csv"),
            "m_regular_season_compact_results"
        );
        assert_eq!(
            derive_table_name("SampleSubmissionStage1.csv"),
            "sample_submission_stage1"
        );
        assert_eq!(derive_table_name("WTeams.csv"), "w_teams");
        assert_eq!(derive_table_name("MSeasons.csv"), "m_seasons");
        assert_eq!(
            derive_table_name("MTeamCoaches.csv"),
            "m_team_coaches"
        );
        assert_eq!(derive_table_name("Conferences.csv"), "conferences");
    }

    #[test]
    fn test_camel_to_snake() {
        assert_eq!(camel_to_snake("MTeams"), "m_teams");
        assert_eq!(camel_to_snake("NCAA"), "ncaa");
        // Runs of uppercase are kept together — M+NCAA merges
        assert_eq!(camel_to_snake("MNCAAFoo"), "mncaa_foo");
        assert_eq!(camel_to_snake("ABCDef"), "abc_def");
        assert_eq!(camel_to_snake("already_snake"), "already_snake");
    }

    #[test]
    fn test_sanitize_column_name() {
        assert_eq!(sanitize_column_name("TeamID"), "teamid");
        assert_eq!(sanitize_column_name("Team Name"), "team_name");
        assert_eq!(sanitize_column_name("1stPlace"), "_1stplace");
        assert_eq!(sanitize_column_name("foo-bar"), "foo_bar");
    }

    #[test]
    fn test_deduplicate_columns() {
        let cols = vec![
            "a".to_string(),
            "b".to_string(),
            "a".to_string(),
            "c".to_string(),
            "a".to_string(),
        ];
        let deduped = deduplicate_columns(&cols);
        assert_eq!(deduped, vec!["a", "b", "a_2", "c", "a_3"]);
    }
}
