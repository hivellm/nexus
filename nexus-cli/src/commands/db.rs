use anyhow::Result;
use clap::{Args, Subcommand};

use super::OutputContext;
use crate::client::NexusClient;

#[derive(Args)]
pub struct DbArgs {
    #[command(subcommand)]
    pub command: DbCommands,
}

#[derive(Subcommand)]
pub enum DbCommands {
    /// Show current database information
    Info,
    /// Clear all data from the database
    Clear {
        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },
    /// Ping the database server
    Ping,
    /// List all databases
    List,
    /// Create a new database
    Create {
        /// Database name
        name: String,
    },
    /// Switch to a different database
    Switch {
        /// Database name
        name: String,
    },
    /// Drop a database
    Drop {
        /// Database name
        name: String,
        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },
}

pub async fn execute(client: &NexusClient, args: DbArgs, output: &OutputContext) -> Result<()> {
    match args.command {
        DbCommands::Info => db_info(client, output).await,
        DbCommands::Clear { force } => clear_db(client, force, output).await,
        DbCommands::Ping => ping(client, output).await,
        DbCommands::List => list_databases(client, output).await,
        DbCommands::Create { name } => create_database(client, &name, output).await,
        DbCommands::Switch { name } => switch_database(client, &name, output).await,
        DbCommands::Drop { name, force } => drop_database(client, &name, force, output).await,
    }
}

async fn db_info(client: &NexusClient, output: &OutputContext) -> Result<()> {
    let stats = client.stats().await?;

    if output.json {
        output.print_json(&stats);
        return Ok(());
    }

    println!("Database Information");
    println!("====================");
    println!("Nodes:         {}", stats.node_count);
    println!("Relationships: {}", stats.relationship_count);
    println!("Labels:        {}", stats.label_count);
    println!("Property Keys: {}", stats.property_key_count);

    Ok(())
}

async fn clear_db(client: &NexusClient, force: bool, output: &OutputContext) -> Result<()> {
    if !force {
        use colored::Colorize;
        use dialoguer::Confirm;

        println!(
            "{}",
            "WARNING: This will delete ALL data from the database!"
                .red()
                .bold()
        );

        let confirmed = Confirm::new()
            .with_prompt("Are you sure you want to continue?")
            .default(false)
            .interact()?;

        if !confirmed {
            output.print_info("Operation cancelled");
            return Ok(());
        }
    }

    client.clear_database().await?;
    output.print_success("Database cleared successfully");

    Ok(())
}

async fn ping(client: &NexusClient, output: &OutputContext) -> Result<()> {
    let start = std::time::Instant::now();
    let ok = client.ping().await?;
    let elapsed = start.elapsed();

    if ok {
        output.print_success(&format!(
            "Server is reachable (response time: {:.2}ms)",
            elapsed.as_secs_f64() * 1000.0
        ));
    } else {
        output.print_error("Server is not reachable");
    }

    Ok(())
}

async fn list_databases(client: &NexusClient, output: &OutputContext) -> Result<()> {
    // Use Cypher SHOW DATABASES command
    match client.query("SHOW DATABASES", None).await {
        Ok(result) => {
            if output.json {
                // Convert to JSON-friendly format
                let json_data = serde_json::json!({
                    "columns": result.columns,
                    "rows": result.rows
                });
                output.print_json(&json_data);
                return Ok(());
            }

            if result.rows.is_empty() {
                // If SHOW DATABASES returns empty, show default
                println!("Databases:");
                println!("  * nexus (default, current)");
            } else {
                output.print_table(&result.columns, &result.rows);
            }
        }
        Err(_) => {
            // SHOW DATABASES not supported, show default database
            if output.json {
                let json_data = serde_json::json!({
                    "databases": [{"name": "nexus", "status": "online", "current": true}]
                });
                output.print_json(&json_data);
            } else {
                println!("Databases:");
                println!("  * nexus (default, current)");
                output.print_info("Multi-database support requires Nexus Enterprise Edition");
            }
        }
    }

    Ok(())
}

async fn create_database(client: &NexusClient, name: &str, output: &OutputContext) -> Result<()> {
    use super::create_spinner;

    let spinner = create_spinner(&format!("Creating database '{}'...", name));

    // Use Cypher CREATE DATABASE command
    let cypher = format!("CREATE DATABASE {}", name);
    match client.query(&cypher, None).await {
        Ok(_) => {
            spinner.finish_and_clear();
            output.print_success(&format!("Database '{}' created successfully", name));
        }
        Err(e) => {
            spinner.finish_and_clear();
            let error_msg = e.to_string();
            if error_msg.contains("not supported") || error_msg.contains("Unsupported") {
                output.print_error("Multi-database support is not available in this edition");
                output.print_info(
                    "This feature requires Nexus Enterprise or Neo4j Enterprise Edition",
                );
            } else {
                return Err(e);
            }
        }
    }

    Ok(())
}

async fn switch_database(client: &NexusClient, name: &str, output: &OutputContext) -> Result<()> {
    // Use Cypher :USE command
    let cypher = format!(":USE {}", name);
    match client.query(&cypher, None).await {
        Ok(_) => {
            output.print_success(&format!("Switched to database '{}'", name));
        }
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("not supported") || error_msg.contains("Unsupported") {
                output.print_error("Multi-database support is not available in this edition");
                output.print_info(
                    "This feature requires Nexus Enterprise or Neo4j Enterprise Edition",
                );
            } else {
                return Err(e);
            }
        }
    }

    Ok(())
}

async fn drop_database(
    client: &NexusClient,
    name: &str,
    force: bool,
    output: &OutputContext,
) -> Result<()> {
    if !force {
        use colored::Colorize;
        use dialoguer::Confirm;

        println!(
            "{}",
            format!("WARNING: This will permanently delete database '{}'!", name)
                .red()
                .bold()
        );

        let confirmed = Confirm::new()
            .with_prompt("Are you sure you want to continue?")
            .default(false)
            .interact()?;

        if !confirmed {
            output.print_info("Operation cancelled");
            return Ok(());
        }
    }

    // Use Cypher DROP DATABASE command
    let cypher = format!("DROP DATABASE {}", name);
    match client.query(&cypher, None).await {
        Ok(_) => {
            output.print_success(&format!("Database '{}' dropped successfully", name));
        }
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("not supported") || error_msg.contains("Unsupported") {
                output.print_error("Multi-database support is not available in this edition");
                output.print_info(
                    "This feature requires Nexus Enterprise or Neo4j Enterprise Edition",
                );
            } else {
                return Err(e);
            }
        }
    }

    Ok(())
}
