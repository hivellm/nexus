use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use serde::Deserialize;

use super::OutputContext;
use crate::client::NexusClient;

#[derive(Args)]
pub struct AdminArgs {
    #[command(subcommand)]
    pub command: AdminCommands,
}

#[derive(Subcommand)]
pub enum AdminCommands {
    /// Check server status
    Status,
    /// Check server health
    Health,
    /// Show database statistics
    Stats,
    /// Encryption-at-rest operator surface
    #[command(subcommand)]
    Encryption(EncryptionCommand),
}

/// `nexus admin encryption …` subcommands. Today the only entry is
/// `status`; migration / rotation / KMS lifecycle ships alongside
/// the corresponding storage-hook follow-ups so the CLI does not
/// expose actions the server can't yet honour.
#[derive(Subcommand)]
pub enum EncryptionCommand {
    /// Show the boot-time encryption-at-rest configuration:
    /// enabled flag, KeyProvider source, master-key fingerprint.
    /// Reads `GET /admin/encryption/status` on the server.
    Status,
}

pub async fn execute(client: &NexusClient, args: AdminArgs, output: &OutputContext) -> Result<()> {
    match args.command {
        AdminCommands::Status => server_status(client, output).await,
        AdminCommands::Health => health_check(client, output).await,
        AdminCommands::Stats => show_stats(client, output).await,
        AdminCommands::Encryption(cmd) => match cmd {
            EncryptionCommand::Status => encryption_status(client, output).await,
        },
    }
}

async fn server_status(client: &NexusClient, output: &OutputContext) -> Result<()> {
    let status = client.status().await?;

    if output.json {
        output.print_json(&status);
        return Ok(());
    }

    use colored::Colorize;

    println!("Server Status");
    println!("=============");
    println!(
        "Status:  {}",
        if status.status == "running" {
            status.status.green()
        } else {
            status.status.red()
        }
    );
    if let Some(version) = status.version {
        println!("Version: {}", version);
    }
    if let Some(uptime) = status.uptime_seconds {
        let hours = uptime / 3600;
        let minutes = (uptime % 3600) / 60;
        let seconds = uptime % 60;
        println!("Uptime:  {}h {}m {}s", hours, minutes, seconds);
    }

    Ok(())
}

async fn health_check(client: &NexusClient, output: &OutputContext) -> Result<()> {
    let health = client.health().await?;

    if output.json {
        output.print_json(&health);
        return Ok(());
    }

    output.print_success("Server is healthy");
    println!("{}", serde_json::to_string_pretty(&health)?);

    Ok(())
}

/// Mirrors the server's `EncryptionStatusReport`. Kept here to
/// avoid pulling `nexus-server` into the CLI dependency closure
/// (the binary stays slim).
#[derive(Debug, Deserialize)]
struct EncryptionStatusReport {
    enabled: bool,
    #[serde(default)]
    source: Option<EncryptionSource>,
    #[serde(default)]
    fingerprint: Option<String>,
    #[serde(default)]
    storage_surfaces: Vec<String>,
    #[serde(default)]
    schema_version: u32,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum EncryptionSource {
    Env { name: String },
    File { path: String },
}

async fn encryption_status(client: &NexusClient, output: &OutputContext) -> Result<()> {
    let report: EncryptionStatusReport = client
        .get_json("/admin/encryption/status")
        .await
        .context("calling /admin/encryption/status")?;

    if output.json {
        output.print_json(&serde_json::json!({
            "enabled": report.enabled,
            "source": report.source.as_ref().map(|s| match s {
                EncryptionSource::Env { name } => serde_json::json!({"kind": "env", "name": name}),
                EncryptionSource::File { path } => serde_json::json!({"kind": "file", "path": path}),
            }),
            "fingerprint": report.fingerprint,
            "storage_surfaces": report.storage_surfaces,
            "schema_version": report.schema_version,
        }));
        return Ok(());
    }

    use colored::Colorize;
    println!("Encryption at rest");
    println!("==================");
    if report.enabled {
        println!("Enabled:           {}", "yes".green());
    } else {
        println!("Enabled:           {}", "no".yellow());
        println!();
        println!(
            "Server is running in plaintext mode. Set NEXUS_ENCRYPT_AT_REST=true \
             and provide NEXUS_DATA_KEY (32 raw bytes / 64 hex chars) or \
             NEXUS_KEY_FILE to enable."
        );
        return Ok(());
    }
    match report.source.as_ref() {
        Some(EncryptionSource::Env { name }) => {
            println!("Source:            env ({name})");
        }
        Some(EncryptionSource::File { path }) => {
            println!("Source:            file ({path})");
        }
        None => println!("Source:            (unknown)"),
    }
    if let Some(fp) = report.fingerprint.as_deref() {
        println!("Master fingerprint: {fp}");
    }
    if report.storage_surfaces.is_empty() {
        println!();
        println!(
            "Storage surfaces:  (none yet) — wiring lands with \
             phase8_encryption-at-rest-storage-hooks / -wal / -indexes."
        );
    } else {
        println!("Storage surfaces:  {}", report.storage_surfaces.join(", "));
    }
    Ok(())
}

async fn show_stats(client: &NexusClient, output: &OutputContext) -> Result<()> {
    let stats = client.stats().await?;

    if output.json {
        output.print_json(&stats);
        return Ok(());
    }

    println!("Database Statistics");
    println!("===================");
    println!("Nodes:              {}", stats.node_count);
    println!("Relationships:      {}", stats.relationship_count);
    println!("Labels:             {}", stats.label_count);
    println!("Property Keys:      {}", stats.property_key_count);

    Ok(())
}
