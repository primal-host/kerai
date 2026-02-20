/// Configuration for the serve subcommand.

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub listen_addr: String,
    pub static_dir: Option<String>,
}
