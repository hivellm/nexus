use anyhow::Result;
use clap::{Args, Subcommand};
use serde_json::Value;

use super::OutputContext;
use crate::client::NexusClient;

#[derive(Args)]
pub struct KeyArgs {
    #[command(subcommand)]
    pub command: KeyCommands,
}

#[derive(Subcommand)]
pub enum KeyCommands {
    /// List all API keys
    List,
    /// Create a new API key
    Create {
        /// Key name
        name: String,
        /// Permissions (comma-separated)
        #[arg(short, long)]
        permissions: Option<String>,
        /// Rate limit (requests per minute)
        #[arg(short, long)]
        rate_limit: Option<u32>,
        /// Expiration time (e.g., "30d", "24h", "never")
        #[arg(short, long)]
        expires: Option<String>,
    },
    /// Get API key information
    Get {
        /// Key ID
        id: String,
    },
    /// Rotate an API key (revoke old and create new)
    Rotate {
        /// Key ID to rotate
        id: String,
        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },
    /// Revoke an API key
    Revoke {
        /// Key ID
        id: String,
        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },
}

pub async fn execute(client: &NexusClient, args: KeyArgs, output: &OutputContext) -> Result<()> {
    match args.command {
        KeyCommands::List => list_keys(client, output).await,
        KeyCommands::Create {
            name,
            permissions,
            rate_limit,
            expires,
        } => {
            create_key(
                client,
                &name,
                permissions.as_deref(),
                rate_limit,
                expires.as_deref(),
                output,
            )
            .await
        }
        KeyCommands::Get { id } => get_key(client, &id, output).await,
        KeyCommands::Rotate { id, force } => rotate_key(client, &id, force, output).await,
        KeyCommands::Revoke { id, force } => revoke_key(client, &id, force, output).await,
    }
}

async fn list_keys(client: &NexusClient, output: &OutputContext) -> Result<()> {
    let keys = client.get_api_keys().await?;

    if output.json {
        output.print_json(&keys);
        return Ok(());
    }

    let columns = vec![
        "ID".to_string(),
        "Name".to_string(),
        "Permissions".to_string(),
        "Active".to_string(),
        "Expires".to_string(),
    ];

    let rows: Vec<Vec<Value>> = keys
        .iter()
        .map(|k| {
            vec![
                Value::String(k.id.clone()),
                Value::String(k.name.clone()),
                Value::String(k.permissions.join(", ")),
                Value::Bool(k.is_active),
                Value::String(k.expires_at.clone().unwrap_or_else(|| "Never".to_string())),
            ]
        })
        .collect();

    output.print_table(&columns, &rows);
    output.print_info(&format!("{} key(s) found", keys.len()));

    Ok(())
}

async fn create_key(
    client: &NexusClient,
    name: &str,
    permissions: Option<&str>,
    rate_limit: Option<u32>,
    expires: Option<&str>,
    output: &OutputContext,
) -> Result<()> {
    let perms_vec: Vec<String> = permissions
        .map(|p| p.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();

    let result = client.create_api_key(name, &perms_vec).await?;

    if output.json {
        output.print_json(&result);
    } else {
        use colored::Colorize;
        output.print_success(&format!("API key '{}' created successfully", name));
        println!();
        println!("  {} {}", "Key ID:".dimmed(), result.id);
        println!("  {} {}", "API Key:".dimmed(), result.key.yellow().bold());
        if let Some(limit) = rate_limit {
            println!("  {} {} requests/min", "Rate Limit:".dimmed(), limit);
        }
        if let Some(exp) = expires {
            if exp != "never" {
                println!("  {} {}", "Expires:".dimmed(), exp);
            }
        }
        println!();
        println!(
            "{}",
            "IMPORTANT: Save this key now. It will not be shown again.".red()
        );
    }

    // Note: rate_limit and expires are displayed but actual enforcement
    // depends on server-side implementation in the auth middleware

    Ok(())
}

async fn get_key(client: &NexusClient, id: &str, output: &OutputContext) -> Result<()> {
    let keys = client.get_api_keys().await?;
    let key = keys.iter().find(|k| k.id == id);

    match key {
        Some(k) => {
            if output.json {
                output.print_json(k);
            } else {
                println!("ID:          {}", k.id);
                println!("Name:        {}", k.name);
                println!("Permissions: {}", k.permissions.join(", "));
                println!("Active:      {}", k.is_active);
                if let Some(expires) = &k.expires_at {
                    println!("Expires:     {}", expires);
                }
                if let Some(created) = &k.created_at {
                    println!("Created:     {}", created);
                }
            }
        }
        None => {
            output.print_error(&format!("API key '{}' not found", id));
        }
    }

    Ok(())
}

async fn rotate_key(
    client: &NexusClient,
    id: &str,
    force: bool,
    output: &OutputContext,
) -> Result<()> {
    // Get the existing key info first
    let keys = client.get_api_keys().await?;
    let existing_key = keys.iter().find(|k| k.id == id);

    let (name, permissions) = match existing_key {
        Some(k) => (k.name.clone(), k.permissions.clone()),
        None => {
            output.print_error(&format!("API key '{}' not found", id));
            return Ok(());
        }
    };

    if !force {
        use dialoguer::Confirm;
        let confirmed = Confirm::new()
            .with_prompt(format!(
                "This will revoke key '{}' and create a new one. Continue?",
                name
            ))
            .default(false)
            .interact()?;

        if !confirmed {
            output.print_info("Operation cancelled");
            return Ok(());
        }
    }

    // Revoke the old key
    client.revoke_api_key(id).await?;
    output.print_info(&format!("Old key '{}' revoked", id));

    // Create a new key with the same name and permissions
    let result = client.create_api_key(&name, &permissions).await?;

    if output.json {
        output.print_json(&result);
    } else {
        use colored::Colorize;
        output.print_success(&format!("API key '{}' rotated successfully", name));
        println!();
        println!("  {} {}", "New Key ID:".dimmed(), result.id);
        println!("  {} {}", "API Key:".dimmed(), result.key.yellow().bold());
        println!();
        println!(
            "{}",
            "IMPORTANT: Save this key now. It will not be shown again.".red()
        );
    }

    Ok(())
}

async fn revoke_key(
    client: &NexusClient,
    id: &str,
    force: bool,
    output: &OutputContext,
) -> Result<()> {
    if !force {
        use dialoguer::Confirm;
        let confirmed = Confirm::new()
            .with_prompt(format!("Are you sure you want to revoke API key '{}'?", id))
            .default(false)
            .interact()?;

        if !confirmed {
            output.print_info("Operation cancelled");
            return Ok(());
        }
    }

    client.revoke_api_key(id).await?;
    output.print_success(&format!("API key '{}' revoked successfully", id));

    Ok(())
}
