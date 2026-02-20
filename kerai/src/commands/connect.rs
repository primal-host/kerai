use postgres::NoTls;

use crate::home;
use crate::output::OutputFormat;

pub fn run(connection: &str, format: &OutputFormat) -> Result<(), String> {
    let home = home::ensure_home_dir()?;
    home::set_kerai_value(&home, "postgres.global.connection", connection)?;

    // Test the connection
    let mut client =
        postgres::Client::connect(connection, NoTls).map_err(|e| format!("Connection failed: {e}"))?;

    // Quick sanity check
    client
        .simple_query("SELECT 1")
        .map_err(|e| format!("Connection test failed: {e}"))?;

    // Seed default aliases into postgres (idempotent via upsert)
    let default_aliases = [("pg", "postgres")];
    for (name, target) in &default_aliases {
        let _ = client.execute(
            "SELECT kerai.set_preference('alias', $1, $2)",
            &[name, target],
        );
    }

    // Sync aliases cache from postgres
    if let Err(e) = super::config_cmd::sync_aliases_from_db(&mut client) {
        eprintln!("Warning: failed to sync aliases cache: {e}");
    }

    let path = home.join("kerai.kerai");
    match format {
        OutputFormat::Json => {
            println!(
                r#"{{"status":"ok","connection":"{}","config":"{}"}}"#,
                connection,
                path.display()
            );
        }
        _ => {
            println!("Connection saved to {}", path.display());
            println!("Connected to {connection}");
        }
    }
    Ok(())
}
