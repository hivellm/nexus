use anyhow::Result;
use clap::{Args, Subcommand};

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
}

pub async fn execute(client: &NexusClient, args: AdminArgs, output: &OutputContext) -> Result<()> {
    match args.command {
        AdminCommands::Status => server_status(client, output).await,
        AdminCommands::Health => health_check(client, output).await,
        AdminCommands::Stats => show_stats(client, output).await,
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
