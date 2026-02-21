/// Pass 2 — Type Promotion: analyze TEXT columns and promote to typed columns.
use pgrx::prelude::*;
use serde_json::{json, Value};
use crate::sql::sql_escape;

/// Per-column statistics collected during promotion.
pub struct ColumnStats {
    pub name: String,
    pub original_name: String,
    pub position: i32,
    pub data_type: String,
    pub has_raw: bool,
    pub nil_count: i64,
    pub empty_count: i64,
    pub unique_count: i64,
    pub min_val: Option<String>,
    pub max_val: Option<String>,
}

impl ColumnStats {
    pub fn to_json(&self) -> Value {
        let mut obj = json!({
            "data_type": self.data_type,
            "original_name": self.original_name,
            "position": self.position,
            "has_raw": self.has_raw,
            "nil_count": self.nil_count,
            "empty_count": self.empty_count,
            "unique_count": self.unique_count,
        });
        if let Some(ref min) = self.min_val {
            obj["min"] = json!(min);
        }
        if let Some(ref max) = self.max_val {
            obj["max"] = json!(max);
        }
        if self.has_raw {
            obj["raw_column"] = json!(format!("{}_raw", self.name));
        }
        let total = self.nil_count + self.empty_count + self.unique_count;
        if total > 0 {
            obj["nil_rate"] = json!((self.nil_count as f64) / (total as f64 + self.nil_count as f64));
        } else {
            obj["nil_rate"] = json!(0.0);
        }
        obj
    }
}

/// Promote all columns in a table from TEXT to their inferred types.
/// Returns per-column stats.
pub fn promote_columns(
    qualified_table: &str,
    columns: &[String],
    original_headers: &[String],
) -> Vec<ColumnStats> {
    let mut stats = Vec::with_capacity(columns.len());

    for (i, col) in columns.iter().enumerate() {
        let original_name = original_headers
            .get(i)
            .cloned()
            .unwrap_or_else(|| col.clone());

        let col_stats = promote_single_column(qualified_table, col, &original_name, i as i32);
        stats.push(col_stats);
    }

    stats
}

/// Analyze and promote a single column.
fn promote_single_column(
    qualified_table: &str,
    col: &str,
    original_name: &str,
    position: i32,
) -> ColumnStats {
    let escaped_col = sql_escape(col);

    // Count NULLs and empties
    let nil_count = Spi::get_one::<i64>(&format!(
        "SELECT COUNT(*) FROM {} WHERE \"{}\" IS NULL",
        qualified_table, escaped_col,
    ))
    .unwrap_or(Some(0))
    .unwrap_or(0);

    let empty_count = Spi::get_one::<i64>(&format!(
        "SELECT COUNT(*) FROM {} WHERE \"{}\" = ''",
        qualified_table, escaped_col,
    ))
    .unwrap_or(Some(0))
    .unwrap_or(0);

    let unique_count = Spi::get_one::<i64>(&format!(
        "SELECT COUNT(DISTINCT \"{}\") FROM {} WHERE \"{}\" IS NOT NULL AND \"{}\" <> ''",
        escaped_col, qualified_table, escaped_col, escaped_col,
    ))
    .unwrap_or(Some(0))
    .unwrap_or(0);

    // Count non-empty values for type testing
    let non_empty_count = Spi::get_one::<i64>(&format!(
        "SELECT COUNT(*) FROM {} WHERE \"{}\" IS NOT NULL AND \"{}\" <> ''",
        qualified_table, escaped_col, escaped_col,
    ))
    .unwrap_or(Some(0))
    .unwrap_or(0);

    // If column is entirely NULL/empty, leave as TEXT
    if non_empty_count == 0 {
        return ColumnStats {
            name: col.to_string(),
            original_name: original_name.to_string(),
            position,
            data_type: "TEXT".to_string(),
            has_raw: false,
            nil_count,
            empty_count,
            unique_count,
            min_val: None,
            max_val: None,
        };
    }

    // Try type promotions in order: integer → float → date → text
    if let Some(stats) = try_promote_integer(qualified_table, col, original_name, position, nil_count, empty_count, unique_count, non_empty_count) {
        return stats;
    }

    if let Some(stats) = try_promote_float(qualified_table, col, original_name, position, nil_count, empty_count, unique_count, non_empty_count) {
        return stats;
    }

    if let Some(stats) = try_promote_date(qualified_table, col, original_name, position, nil_count, empty_count, unique_count, non_empty_count) {
        return stats;
    }

    // Stay as TEXT — collect min/max
    let min_val = Spi::get_one::<String>(&format!(
        "SELECT MIN(\"{}\") FROM {} WHERE \"{}\" IS NOT NULL AND \"{}\" <> ''",
        escaped_col, qualified_table, escaped_col, escaped_col,
    ))
    .unwrap_or(None);

    let max_val = Spi::get_one::<String>(&format!(
        "SELECT MAX(\"{}\") FROM {} WHERE \"{}\" IS NOT NULL AND \"{}\" <> ''",
        escaped_col, qualified_table, escaped_col, escaped_col,
    ))
    .unwrap_or(None);

    ColumnStats {
        name: col.to_string(),
        original_name: original_name.to_string(),
        position,
        data_type: "TEXT".to_string(),
        has_raw: false,
        nil_count,
        empty_count,
        unique_count,
        min_val,
        max_val,
    }
}

fn try_promote_integer(
    qualified_table: &str,
    col: &str,
    original_name: &str,
    position: i32,
    nil_count: i64,
    empty_count: i64,
    unique_count: i64,
    non_empty_count: i64,
) -> Option<ColumnStats> {
    let escaped_col = sql_escape(col);

    // Count values that successfully cast to BIGINT
    let castable = Spi::get_one::<i64>(&format!(
        "SELECT COUNT(*) FROM {} WHERE \"{}\" IS NOT NULL AND \"{}\" <> '' \
         AND \"{}\" ~ '^-?[0-9]+$'",
        qualified_table, escaped_col, escaped_col, escaped_col,
    ))
    .unwrap_or(Some(0))
    .unwrap_or(0);

    if castable == 0 {
        return None;
    }

    let fully_clean = castable == non_empty_count;

    if fully_clean {
        // Clean promotion: all values cast
        do_clean_promote(qualified_table, col, "BIGINT", "BIGINT");

        let (min_val, max_val) = get_minmax_typed(qualified_table, col);

        Some(ColumnStats {
            name: col.to_string(),
            original_name: original_name.to_string(),
            position,
            data_type: "BIGINT".to_string(),
            has_raw: false,
            nil_count,
            empty_count,
            unique_count,
            min_val,
            max_val,
        })
    } else {
        // Partial promotion: keep _raw column
        do_partial_promote(qualified_table, col, "BIGINT", &format!(
            "CASE WHEN \"{}\" ~ '^-?[0-9]+$' THEN \"{}\"::BIGINT ELSE NULL END",
            escaped_col, escaped_col,
        ));

        let (min_val, max_val) = get_minmax_typed(qualified_table, col);

        Some(ColumnStats {
            name: col.to_string(),
            original_name: original_name.to_string(),
            position,
            data_type: "BIGINT".to_string(),
            has_raw: true,
            nil_count,
            empty_count,
            unique_count,
            min_val,
            max_val,
        })
    }
}

fn try_promote_float(
    qualified_table: &str,
    col: &str,
    original_name: &str,
    position: i32,
    nil_count: i64,
    empty_count: i64,
    unique_count: i64,
    non_empty_count: i64,
) -> Option<ColumnStats> {
    let escaped_col = sql_escape(col);

    // Count values that look like floats (including integers)
    let castable = Spi::get_one::<i64>(&format!(
        "SELECT COUNT(*) FROM {} WHERE \"{}\" IS NOT NULL AND \"{}\" <> '' \
         AND \"{}\" ~ '^-?[0-9]+(\\.[0-9]+)?([eE][+-]?[0-9]+)?$'",
        qualified_table, escaped_col, escaped_col, escaped_col,
    ))
    .unwrap_or(Some(0))
    .unwrap_or(0);

    if castable == 0 {
        return None;
    }

    // Only promote to float if at least one value has a decimal point or exponent
    let has_decimal = Spi::get_one::<i64>(&format!(
        "SELECT COUNT(*) FROM {} WHERE \"{}\" IS NOT NULL AND \"{}\" <> '' \
         AND \"{}\" ~ '[.eE]'",
        qualified_table, escaped_col, escaped_col, escaped_col,
    ))
    .unwrap_or(Some(0))
    .unwrap_or(0);

    if has_decimal == 0 {
        return None; // Pure integers — let integer promotion handle it
    }

    let fully_clean = castable == non_empty_count;

    if fully_clean {
        do_clean_promote(qualified_table, col, "DOUBLE PRECISION", "DOUBLE PRECISION");

        let (min_val, max_val) = get_minmax_typed(qualified_table, col);

        Some(ColumnStats {
            name: col.to_string(),
            original_name: original_name.to_string(),
            position,
            data_type: "DOUBLE PRECISION".to_string(),
            has_raw: false,
            nil_count,
            empty_count,
            unique_count,
            min_val,
            max_val,
        })
    } else {
        do_partial_promote(qualified_table, col, "DOUBLE PRECISION", &format!(
            "CASE WHEN \"{}\" ~ '^-?[0-9]+(\\.[0-9]+)?([eE][+-]?[0-9]+)?$' \
             THEN \"{}\"::DOUBLE PRECISION ELSE NULL END",
            escaped_col, escaped_col,
        ));

        let (min_val, max_val) = get_minmax_typed(qualified_table, col);

        Some(ColumnStats {
            name: col.to_string(),
            original_name: original_name.to_string(),
            position,
            data_type: "DOUBLE PRECISION".to_string(),
            has_raw: true,
            nil_count,
            empty_count,
            unique_count,
            min_val,
            max_val,
        })
    }
}

fn try_promote_date(
    qualified_table: &str,
    col: &str,
    original_name: &str,
    position: i32,
    nil_count: i64,
    empty_count: i64,
    unique_count: i64,
    non_empty_count: i64,
) -> Option<ColumnStats> {
    let escaped_col = sql_escape(col);

    // Check if values match common date patterns: MM/DD/YYYY, YYYY-MM-DD
    let castable = Spi::get_one::<i64>(&format!(
        "SELECT COUNT(*) FROM {} WHERE \"{}\" IS NOT NULL AND \"{}\" <> '' \
         AND (\"{}\" ~ '^[0-9]{{1,2}}/[0-9]{{1,2}}/[0-9]{{4}}$' \
              OR \"{}\" ~ '^[0-9]{{4}}-[0-9]{{2}}-[0-9]{{2}}$')",
        qualified_table, escaped_col, escaped_col, escaped_col, escaped_col,
    ))
    .unwrap_or(Some(0))
    .unwrap_or(0);

    if castable == 0 {
        return None;
    }

    let fully_clean = castable == non_empty_count;

    if fully_clean {
        // Use DATE type with safe casting
        do_date_promote_clean(qualified_table, col);

        let (min_val, max_val) = get_minmax_typed(qualified_table, col);

        Some(ColumnStats {
            name: col.to_string(),
            original_name: original_name.to_string(),
            position,
            data_type: "DATE".to_string(),
            has_raw: false,
            nil_count,
            empty_count,
            unique_count,
            min_val,
            max_val,
        })
    } else {
        do_date_promote_partial(qualified_table, col);

        let (min_val, max_val) = get_minmax_typed(qualified_table, col);

        Some(ColumnStats {
            name: col.to_string(),
            original_name: original_name.to_string(),
            position,
            data_type: "DATE".to_string(),
            has_raw: true,
            nil_count,
            empty_count,
            unique_count,
            min_val,
            max_val,
        })
    }
}

/// Clean promotion: rename original TEXT col, add typed col, copy data, drop old.
fn do_clean_promote(qualified_table: &str, col: &str, pg_type: &str, cast_type: &str) {
    let escaped = sql_escape(col);
    let tmp = format!("{}_tmp_text", escaped);

    Spi::run(&format!(
        "ALTER TABLE {} RENAME COLUMN \"{}\" TO \"{}\"",
        qualified_table, escaped, tmp,
    )).expect("Failed to rename column for promotion");

    Spi::run(&format!(
        "ALTER TABLE {} ADD COLUMN \"{}\" {}",
        qualified_table, escaped, pg_type,
    )).expect("Failed to add typed column");

    Spi::run(&format!(
        "UPDATE {} SET \"{}\" = CASE WHEN \"{}\" IS NOT NULL AND \"{}\" <> '' \
         THEN \"{}\"::{} ELSE NULL END",
        qualified_table, escaped, tmp, tmp, tmp, cast_type,
    )).expect("Failed to populate typed column");

    Spi::run(&format!(
        "ALTER TABLE {} DROP COLUMN \"{}\"",
        qualified_table, tmp,
    )).expect("Failed to drop old text column");
}

/// Partial promotion: keep _raw TEXT column, add typed column.
fn do_partial_promote(qualified_table: &str, col: &str, pg_type: &str, cast_expr: &str) {
    let escaped = sql_escape(col);
    let raw_col = format!("{}_raw", escaped);

    // Rename original TEXT → _raw
    Spi::run(&format!(
        "ALTER TABLE {} RENAME COLUMN \"{}\" TO \"{}\"",
        qualified_table, escaped, raw_col,
    )).expect("Failed to rename column to _raw");

    // Add typed column
    Spi::run(&format!(
        "ALTER TABLE {} ADD COLUMN \"{}\" {}",
        qualified_table, escaped, pg_type,
    )).expect("Failed to add typed column");

    // Populate typed column using the _raw column
    // Replace col references in cast_expr with _raw
    let raw_cast_expr = cast_expr.replace(
        &format!("\"{}\"", escaped),
        &format!("\"{}\"", raw_col),
    );

    Spi::run(&format!(
        "UPDATE {} SET \"{}\" = {}",
        qualified_table, escaped, raw_cast_expr,
    )).expect("Failed to populate typed column");
}

/// Clean date promotion using safe date conversion.
fn do_date_promote_clean(qualified_table: &str, col: &str) {
    let escaped = sql_escape(col);
    let tmp = format!("{}_tmp_text", escaped);

    Spi::run(&format!(
        "ALTER TABLE {} RENAME COLUMN \"{}\" TO \"{}\"",
        qualified_table, escaped, tmp,
    )).expect("Failed to rename column for date promotion");

    Spi::run(&format!(
        "ALTER TABLE {} ADD COLUMN \"{}\" DATE",
        qualified_table, escaped,
    )).expect("Failed to add date column");

    // Handle MM/DD/YYYY and YYYY-MM-DD formats
    Spi::run(&format!(
        "UPDATE {} SET \"{}\" = CASE \
         WHEN \"{}\" ~ '^[0-9]{{1,2}}/[0-9]{{1,2}}/[0-9]{{4}}$' \
           THEN to_date(\"{}\", 'MM/DD/YYYY') \
         WHEN \"{}\" ~ '^[0-9]{{4}}-[0-9]{{2}}-[0-9]{{2}}$' \
           THEN \"{}\"::DATE \
         ELSE NULL END",
        qualified_table, escaped, tmp, tmp, tmp, tmp,
    )).expect("Failed to populate date column");

    Spi::run(&format!(
        "ALTER TABLE {} DROP COLUMN \"{}\"",
        qualified_table, tmp,
    )).expect("Failed to drop old text column");
}

/// Partial date promotion: keep _raw TEXT column.
fn do_date_promote_partial(qualified_table: &str, col: &str) {
    let escaped = sql_escape(col);
    let raw_col = format!("{}_raw", escaped);

    Spi::run(&format!(
        "ALTER TABLE {} RENAME COLUMN \"{}\" TO \"{}\"",
        qualified_table, escaped, raw_col,
    )).expect("Failed to rename column to _raw");

    Spi::run(&format!(
        "ALTER TABLE {} ADD COLUMN \"{}\" DATE",
        qualified_table, escaped,
    )).expect("Failed to add date column");

    Spi::run(&format!(
        "UPDATE {} SET \"{}\" = CASE \
         WHEN \"{}\" ~ '^[0-9]{{1,2}}/[0-9]{{1,2}}/[0-9]{{4}}$' \
           THEN to_date(\"{}\", 'MM/DD/YYYY') \
         WHEN \"{}\" ~ '^[0-9]{{4}}-[0-9]{{2}}-[0-9]{{2}}$' \
           THEN \"{}\"::DATE \
         ELSE NULL END",
        qualified_table, escaped, raw_col, raw_col, raw_col, raw_col,
    )).expect("Failed to populate date column");
}

/// Get min/max values from a promoted (typed) column as strings.
fn get_minmax_typed(qualified_table: &str, col: &str) -> (Option<String>, Option<String>) {
    let escaped = sql_escape(col);

    let min_val = Spi::get_one::<String>(&format!(
        "SELECT MIN(\"{}\")::text FROM {} WHERE \"{}\" IS NOT NULL",
        escaped, qualified_table, escaped,
    ))
    .unwrap_or(None);

    let max_val = Spi::get_one::<String>(&format!(
        "SELECT MAX(\"{}\")::text FROM {} WHERE \"{}\" IS NOT NULL",
        escaped, qualified_table, escaped,
    ))
    .unwrap_or(None);

    (min_val, max_val)
}
