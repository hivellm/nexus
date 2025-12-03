use anyhow::Result;
use clap::{Args, Subcommand};
use serde_json::Value;

use super::OutputContext;
use crate::client::NexusClient;

#[derive(Args)]
pub struct SchemaArgs {
    #[command(subcommand)]
    pub command: SchemaCommands,
}

#[derive(Subcommand)]
pub enum SchemaCommands {
    /// Manage labels
    Labels {
        #[command(subcommand)]
        command: LabelsCommands,
    },
    /// Manage relationship types
    Types {
        #[command(subcommand)]
        command: TypesCommands,
    },
    /// Manage indexes
    Indexes {
        #[command(subcommand)]
        command: IndexesCommands,
    },
}

#[derive(Subcommand)]
pub enum LabelsCommands {
    /// List all labels
    List,
    /// Create a label (by creating a node with that label)
    Create {
        /// Label name
        name: String,
    },
    /// Delete all nodes with a label
    Delete {
        /// Label name
        name: String,
        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Subcommand)]
pub enum TypesCommands {
    /// List all relationship types
    List,
    /// Create a relationship type (by creating a relationship)
    Create {
        /// Relationship type name
        name: String,
    },
    /// Delete all relationships of a type
    Delete {
        /// Relationship type name
        name: String,
        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Subcommand)]
pub enum IndexesCommands {
    /// List all indexes
    List,
    /// Create an index
    Create {
        /// Label name
        #[arg(short, long)]
        label: String,
        /// Property name
        #[arg(short, long)]
        property: String,
        /// Index name (optional)
        #[arg(short, long)]
        name: Option<String>,
    },
    /// Delete an index
    Delete {
        /// Index name
        name: String,
        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },
}

pub async fn execute(client: &NexusClient, args: SchemaArgs, output: &OutputContext) -> Result<()> {
    match args.command {
        SchemaCommands::Labels { command } => match command {
            LabelsCommands::List => list_labels(client, output).await,
            LabelsCommands::Create { name } => create_label(client, &name, output).await,
            LabelsCommands::Delete { name, force } => {
                delete_label(client, &name, force, output).await
            }
        },
        SchemaCommands::Types { command } => match command {
            TypesCommands::List => list_types(client, output).await,
            TypesCommands::Create { name } => create_type(client, &name, output).await,
            TypesCommands::Delete { name, force } => {
                delete_type(client, &name, force, output).await
            }
        },
        SchemaCommands::Indexes { command } => match command {
            IndexesCommands::List => list_indexes(client, output).await,
            IndexesCommands::Create {
                label,
                property,
                name,
            } => create_index(client, &label, &property, name.as_deref(), output).await,
            IndexesCommands::Delete { name, force } => {
                delete_index(client, &name, force, output).await
            }
        },
    }
}

async fn list_labels(client: &NexusClient, output: &OutputContext) -> Result<()> {
    let labels = client.get_labels().await?;

    if output.json {
        output.print_json(&labels);
        return Ok(());
    }

    let columns = vec!["Label".to_string()];
    let rows: Vec<Vec<Value>> = labels
        .iter()
        .map(|l| vec![Value::String(l.clone())])
        .collect();

    output.print_table(&columns, &rows);
    output.print_info(&format!("{} label(s) found", labels.len()));

    Ok(())
}

async fn list_types(client: &NexusClient, output: &OutputContext) -> Result<()> {
    let types = client.get_relationship_types().await?;

    if output.json {
        output.print_json(&types);
        return Ok(());
    }

    let columns = vec!["Relationship Type".to_string()];
    let rows: Vec<Vec<Value>> = types
        .iter()
        .map(|t| vec![Value::String(t.clone())])
        .collect();

    output.print_table(&columns, &rows);
    output.print_info(&format!("{} relationship type(s) found", types.len()));

    Ok(())
}

async fn list_indexes(client: &NexusClient, output: &OutputContext) -> Result<()> {
    let indexes = client.get_indexes().await?;

    if output.json {
        output.print_json(&indexes);
        return Ok(());
    }

    if indexes.is_empty() {
        output.print_info("No indexes found");
        return Ok(());
    }

    let columns = vec!["Index".to_string()];
    let rows: Vec<Vec<Value>> = indexes.iter().map(|i| vec![i.clone()]).collect();

    output.print_table(&columns, &rows);
    output.print_info(&format!("{} index(es) found", indexes.len()));

    Ok(())
}

async fn create_label(client: &NexusClient, name: &str, output: &OutputContext) -> Result<()> {
    // Create a temporary node with the label, then delete it
    // This ensures the label exists in the schema
    let cypher = format!(
        "CREATE (n:{} {{_temp: true}}) WITH n DELETE n RETURN '{}'",
        name, name
    );
    client.query(&cypher, None).await?;
    output.print_success(&format!("Label '{}' created (or already exists)", name));
    Ok(())
}

async fn delete_label(
    client: &NexusClient,
    name: &str,
    force: bool,
    output: &OutputContext,
) -> Result<()> {
    // Get count first
    let count_cypher = format!("MATCH (n:{}) RETURN count(n) as count", name);
    let result = client.query(&count_cypher, None).await?;
    let count = result
        .rows
        .first()
        .and_then(|r| r.first())
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    if count == 0 {
        output.print_info(&format!("No nodes with label '{}' found", name));
        return Ok(());
    }

    if !force {
        use dialoguer::Confirm;
        let confirmed = Confirm::new()
            .with_prompt(format!(
                "This will delete {} node(s) with label '{}'. Continue?",
                count, name
            ))
            .default(false)
            .interact()?;

        if !confirmed {
            output.print_info("Operation cancelled");
            return Ok(());
        }
    }

    let delete_cypher = format!("MATCH (n:{}) DETACH DELETE n", name);
    client.query(&delete_cypher, None).await?;
    output.print_success(&format!("Deleted {} node(s) with label '{}'", count, name));
    Ok(())
}

async fn create_type(client: &NexusClient, name: &str, output: &OutputContext) -> Result<()> {
    // Create two temporary nodes and a relationship between them
    let cypher = format!(
        "CREATE (a:_Temp)-[r:{}]->(b:_Temp) WITH a, r, b DELETE a, r, b RETURN '{}'",
        name, name
    );
    client.query(&cypher, None).await?;
    output.print_success(&format!(
        "Relationship type '{}' created (or already exists)",
        name
    ));
    Ok(())
}

async fn delete_type(
    client: &NexusClient,
    name: &str,
    force: bool,
    output: &OutputContext,
) -> Result<()> {
    // Get count first
    let count_cypher = format!("MATCH ()-[r:{}]->() RETURN count(r) as count", name);
    let result = client.query(&count_cypher, None).await?;
    let count = result
        .rows
        .first()
        .and_then(|r| r.first())
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    if count == 0 {
        output.print_info(&format!("No relationships of type '{}' found", name));
        return Ok(());
    }

    if !force {
        use dialoguer::Confirm;
        let confirmed = Confirm::new()
            .with_prompt(format!(
                "This will delete {} relationship(s) of type '{}'. Continue?",
                count, name
            ))
            .default(false)
            .interact()?;

        if !confirmed {
            output.print_info("Operation cancelled");
            return Ok(());
        }
    }

    let delete_cypher = format!("MATCH ()-[r:{}]->() DELETE r", name);
    client.query(&delete_cypher, None).await?;
    output.print_success(&format!(
        "Deleted {} relationship(s) of type '{}'",
        count, name
    ));
    Ok(())
}

async fn create_index(
    client: &NexusClient,
    label: &str,
    property: &str,
    name: Option<&str>,
    output: &OutputContext,
) -> Result<()> {
    let cypher = if let Some(index_name) = name {
        format!(
            "CREATE INDEX {} FOR (n:{}) ON (n.{})",
            index_name, label, property
        )
    } else {
        format!("CREATE INDEX FOR (n:{}) ON (n.{})", label, property)
    };

    client.query(&cypher, None).await?;
    output.print_success(&format!("Index created for :{}.{}", label, property));
    Ok(())
}

async fn delete_index(
    client: &NexusClient,
    name: &str,
    force: bool,
    output: &OutputContext,
) -> Result<()> {
    if !force {
        use dialoguer::Confirm;
        let confirmed = Confirm::new()
            .with_prompt(format!("Are you sure you want to delete index '{}'?", name))
            .default(false)
            .interact()?;

        if !confirmed {
            output.print_info("Operation cancelled");
            return Ok(());
        }
    }

    let cypher = format!("DROP INDEX {}", name);
    client.query(&cypher, None).await?;
    output.print_success(&format!("Index '{}' deleted", name));
    Ok(())
}
