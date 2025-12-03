use anyhow::Result;
use clap::{Parser, Subcommand};

mod client;
mod commands;
mod config;
mod cypher_helper;

use commands::{admin, completion, config as config_cmd, data, db, key, query, schema, user};

/// Command-line interface for Nexus Graph Database
#[derive(Parser)]
#[command(name = "nexus")]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Path to configuration file
    #[arg(long, env = "NEXUS_CONFIG")]
    pub config: Option<String>,

    /// Nexus server URL
    #[arg(long, env = "NEXUS_URL")]
    pub url: Option<String>,

    /// API key for authentication
    #[arg(long, env = "NEXUS_API_KEY")]
    pub api_key: Option<String>,

    /// Username for authentication
    #[arg(long, env = "NEXUS_USERNAME")]
    pub username: Option<String>,

    /// Password for authentication
    #[arg(long, env = "NEXUS_PASSWORD")]
    pub password: Option<String>,

    /// Connection profile name
    #[arg(long, env = "NEXUS_PROFILE")]
    pub profile: Option<String>,

    /// Verbose output
    #[arg(short, long)]
    pub verbose: bool,

    /// Debug output
    #[arg(long)]
    pub debug: bool,

    /// Output in JSON format
    #[arg(long)]
    pub json: bool,

    /// Output in CSV format
    #[arg(long)]
    pub csv: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Execute Cypher queries
    Query(query::QueryArgs),
    /// Database management
    Db(db::DbArgs),
    /// User management
    User(user::UserArgs),
    /// API key management
    Key(key::KeyArgs),
    /// Schema operations
    Schema(schema::SchemaArgs),
    /// Data import/export operations
    Data(data::DataArgs),
    /// Administrative operations
    Admin(admin::AdminArgs),
    /// Configuration management
    Config(config_cmd::ConfigArgs),
    /// Generate shell completion scripts
    Completion(completion::CompletionArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load configuration
    let cfg = config::Config::load(cli.config.as_deref())?;

    // Create client with merged options
    let client = client::NexusClient::new(
        cli.url.as_deref().or(cfg.url.as_deref()),
        cli.api_key.as_deref().or(cfg.api_key.as_deref()),
        cli.username.as_deref().or(cfg.username.as_deref()),
        cli.password.as_deref().or(cfg.password.as_deref()),
    )?;

    // Create output context
    let output = commands::OutputContext {
        json: cli.json,
        csv: cli.csv,
        verbose: cli.verbose,
        debug: cli.debug,
    };

    // Execute command
    match cli.command {
        Commands::Query(args) => query::execute(&client, args, &output).await,
        Commands::Db(args) => db::execute(&client, args, &output).await,
        Commands::User(args) => user::execute(&client, args, &output).await,
        Commands::Key(args) => key::execute(&client, args, &output).await,
        Commands::Schema(args) => schema::execute(&client, args, &output).await,
        Commands::Data(args) => data::execute(&client, args, &output).await,
        Commands::Admin(args) => admin::execute(&client, args, &output).await,
        Commands::Config(args) => config_cmd::execute(args, &cfg, &output).await,
        Commands::Completion(args) => completion::execute(args, &output).await,
    }
}
