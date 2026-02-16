use postgres::Client;

use crate::output::{print_json, print_rows, OutputFormat};

pub fn register(
    client: &mut Client,
    pubkey: &str,
    wallet_type: &str,
    label: Option<&str>,
    format: &OutputFormat,
) -> Result<(), String> {
    let row = client
        .query_one(
            "SELECT kerai.register_wallet($1, $2, $3)::text",
            &[&pubkey, &wallet_type, &label],
        )
        .map_err(|e| format!("register_wallet failed: {e}"))?;

    let text: String = row.get(0);
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Invalid JSON: {e}"))?;

    let id = value["id"].as_str().unwrap_or("unknown");
    let fp = value["key_fingerprint"].as_str().unwrap_or("");
    println!("Registered {wallet_type} wallet {id} (fingerprint: {fp})");
    print_json(&value, format);
    Ok(())
}

pub fn transfer(
    client: &mut Client,
    from: &str,
    to: &str,
    amount: i64,
    nonce: i64,
    signature: &str,
    reason: Option<&str>,
    format: &OutputFormat,
) -> Result<(), String> {
    let row = client
        .query_one(
            "SELECT kerai.signed_transfer($1::uuid, $2::uuid, $3, $4, $5, $6)::text",
            &[&from, &to, &amount, &nonce, &signature, &reason],
        )
        .map_err(|e| format!("signed_transfer failed: {e}"))?;

    let text: String = row.get(0);
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Invalid JSON: {e}"))?;

    println!("Signed transfer of {amount} kōi completed");
    print_json(&value, format);
    Ok(())
}

pub fn supply(client: &mut Client, format: &OutputFormat) -> Result<(), String> {
    let row = client
        .query_one("SELECT kerai.supply_info()::text", &[])
        .map_err(|e| format!("supply_info failed: {e}"))?;

    let text: String = row.get(0);
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Invalid JSON: {e}"))?;

    let total = value["total_supply"].as_i64().unwrap_or(0);
    let wallets = value["wallet_count"].as_i64().unwrap_or(0);
    println!("Total supply: {total} kōi across {wallets} wallets");
    print_json(&value, format);
    Ok(())
}

pub fn share(
    client: &mut Client,
    wallet_id: &str,
    format: &OutputFormat,
) -> Result<(), String> {
    let row = client
        .query_one(
            "SELECT kerai.wallet_share($1::uuid)::text",
            &[&wallet_id],
        )
        .map_err(|e| format!("wallet_share failed: {e}"))?;

    let text: String = row.get(0);
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Invalid JSON: {e}"))?;

    let balance = value["balance"].as_i64().unwrap_or(0);
    let share_str = value["share"].as_str().unwrap_or("0");
    println!("Balance: {balance} kōi (share: {share_str})");
    print_json(&value, format);
    Ok(())
}

pub fn schedule(client: &mut Client, format: &OutputFormat) -> Result<(), String> {
    let row = client
        .query_one("SELECT kerai.get_reward_schedule()::text", &[])
        .map_err(|e| format!("get_reward_schedule failed: {e}"))?;

    let text: String = row.get(0);
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Invalid JSON: {e}"))?;

    let arr = value.as_array().ok_or("Expected JSON array")?;

    if arr.is_empty() {
        println!("No reward schedule entries.");
        return Ok(());
    }

    let columns = vec![
        "work_type".into(),
        "reward".into(),
        "enabled".into(),
        "updated".into(),
    ];

    let rows: Vec<Vec<String>> = arr
        .iter()
        .map(|e| {
            vec![
                e["work_type"].as_str().unwrap_or("").to_string(),
                e["reward"]
                    .as_i64()
                    .map(|n| n.to_string())
                    .unwrap_or_default(),
                e["enabled"]
                    .as_bool()
                    .map(|b| b.to_string())
                    .unwrap_or_default(),
                e["updated_at"].as_str().unwrap_or("").to_string(),
            ]
        })
        .collect();

    print_rows(&columns, &rows, format);
    Ok(())
}

pub fn set_reward(
    client: &mut Client,
    work_type: &str,
    reward: i64,
    enabled: Option<bool>,
    format: &OutputFormat,
) -> Result<(), String> {
    let row = client
        .query_one(
            "SELECT kerai.set_reward($1, $2, $3)::text",
            &[&work_type, &reward, &enabled],
        )
        .map_err(|e| format!("set_reward failed: {e}"))?;

    let text: String = row.get(0);
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Invalid JSON: {e}"))?;

    println!("Reward schedule updated for '{work_type}': {reward} kōi");
    print_json(&value, format);
    Ok(())
}
