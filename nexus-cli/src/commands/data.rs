use anyhow::Result;
use clap::{Args, Subcommand};
use std::fs;

use super::OutputContext;
use crate::client::NexusClient;

#[derive(Args)]
pub struct DataArgs {
    #[command(subcommand)]
    pub command: DataCommands,
}

#[derive(Subcommand)]
pub enum DataCommands {
    /// Import data from a file
    Import {
        /// File path
        file: String,
        /// Format (json, csv, cypher)
        #[arg(short, long, default_value = "json")]
        format: String,
        /// Batch size
        #[arg(short, long, default_value = "1000")]
        batch_size: usize,
    },
    /// Export data to a file
    Export {
        /// File path
        file: String,
        /// Format (json, csv, cypher)
        #[arg(short, long, default_value = "json")]
        format: String,
    },
    /// Create a backup of the database
    Backup {
        /// Backup destination path
        destination: String,
        /// Compress the backup
        #[arg(short, long)]
        compress: bool,
    },
    /// Restore database from a backup
    Restore {
        /// Backup source path
        source: String,
        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },
    /// List available backups
    Backups {
        /// Directory to list backups from
        #[arg(short, long)]
        dir: Option<String>,
    },
}

pub async fn execute(client: &NexusClient, args: DataArgs, output: &OutputContext) -> Result<()> {
    match args.command {
        DataCommands::Import {
            file,
            format,
            batch_size,
        } => import_data(client, &file, &format, batch_size, output).await,
        DataCommands::Export { file, format } => export_data(client, &file, &format, output).await,
        DataCommands::Backup {
            destination,
            compress,
        } => backup_data(client, &destination, compress, output).await,
        DataCommands::Restore { source, force } => {
            restore_data(client, &source, force, output).await
        }
        DataCommands::Backups { dir } => list_backups(dir.as_deref(), output).await,
    }
}

async fn import_data(
    client: &NexusClient,
    file: &str,
    format: &str,
    _batch_size: usize,
    output: &OutputContext,
) -> Result<()> {
    use super::create_spinner;

    let spinner = create_spinner(&format!("Reading {}...", file));

    let content = fs::read_to_string(file)?;
    spinner.set_message("Importing data...".to_string());

    client.import_data(&content, format).await?;

    spinner.finish_and_clear();
    output.print_success(&format!(
        "Data imported successfully from {} ({} format)",
        file, format
    ));
    Ok(())
}

async fn export_data(
    client: &NexusClient,
    file: &str,
    format: &str,
    output: &OutputContext,
) -> Result<()> {
    use super::create_spinner;

    let spinner = create_spinner("Exporting data...");

    let data = client.export_data(format).await?;
    spinner.set_message(format!("Writing to {}...", file));
    fs::write(file, &data)?;

    spinner.finish_and_clear();
    output.print_success(&format!("Data exported to {} ({} format)", file, format));
    Ok(())
}

async fn backup_data(
    client: &NexusClient,
    destination: &str,
    compress: bool,
    output: &OutputContext,
) -> Result<()> {
    use super::create_spinner;
    use chrono::Local;
    use std::path::Path;

    // Create backup directory if it doesn't exist
    let dest_path = Path::new(destination);
    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Start spinner
    let spinner = create_spinner("Exporting database...");

    // Export data as JSON (full backup)
    let data = client.export_data("json").await?;
    spinner.set_message("Processing backup...");

    // Generate backup filename with timestamp
    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    let backup_file = if dest_path.is_dir() {
        dest_path.join(format!("nexus_backup_{}.json", timestamp))
    } else {
        dest_path.to_path_buf()
    };

    if compress {
        use flate2::Compression;
        use flate2::write::GzEncoder;
        use std::io::Write;

        spinner.set_message("Compressing backup...");
        let compressed_file = backup_file.with_extension("json.gz");
        let file = fs::File::create(&compressed_file)?;
        let mut encoder = GzEncoder::new(file, Compression::default());
        encoder.write_all(data.as_bytes())?;
        encoder.finish()?;

        spinner.finish_and_clear();
        output.print_success(&format!("Backup created: {}", compressed_file.display()));
    } else {
        spinner.set_message("Writing backup file...");
        fs::write(&backup_file, &data)?;
        spinner.finish_and_clear();
        output.print_success(&format!("Backup created: {}", backup_file.display()));
    }

    Ok(())
}

async fn restore_data(
    client: &NexusClient,
    source: &str,
    force: bool,
    output: &OutputContext,
) -> Result<()> {
    use super::create_spinner;
    use std::path::Path;

    let source_path = Path::new(source);
    if !source_path.exists() {
        anyhow::bail!("Backup file not found: {}", source);
    }

    if !force {
        use dialoguer::Confirm;
        let confirmed = Confirm::new()
            .with_prompt("This will overwrite existing data. Continue?")
            .default(false)
            .interact()?;

        if !confirmed {
            output.print_info("Operation cancelled");
            return Ok(());
        }
    }

    let spinner = create_spinner("Reading backup file...");

    // Read backup data (handle compressed files)
    let data = if source.ends_with(".gz") {
        use flate2::read::GzDecoder;
        use std::io::Read;

        spinner.set_message("Decompressing backup...");
        let file = fs::File::open(source)?;
        let mut decoder = GzDecoder::new(file);
        let mut data = String::new();
        decoder.read_to_string(&mut data)?;
        data
    } else {
        fs::read_to_string(source)?
    };

    spinner.set_message("Restoring database...");

    // Import the data
    client.import_data(&data, "json").await?;

    spinner.finish_and_clear();
    output.print_success("Database restored successfully");
    Ok(())
}

async fn list_backups(dir: Option<&str>, output: &OutputContext) -> Result<()> {
    use std::path::Path;

    let backup_dir = dir.unwrap_or(".");
    let dir_path = Path::new(backup_dir);

    if !dir_path.exists() {
        anyhow::bail!("Directory not found: {}", backup_dir);
    }

    let mut backups: Vec<(String, u64, String)> = Vec::new();

    for entry in fs::read_dir(dir_path)? {
        let entry = entry?;
        let path = entry.path();
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Match backup files
        if filename.starts_with("nexus_backup_")
            && (filename.ends_with(".json") || filename.ends_with(".json.gz"))
        {
            let metadata = entry.metadata()?;
            let size = metadata.len();
            let modified = metadata
                .modified()
                .map(|t| {
                    let datetime: chrono::DateTime<chrono::Local> = t.into();
                    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
                })
                .unwrap_or_else(|_| "Unknown".to_string());

            backups.push((filename.to_string(), size, modified));
        }
    }

    if backups.is_empty() {
        output.print_info(&format!("No backups found in {}", backup_dir));
        return Ok(());
    }

    // Sort by filename (which includes timestamp)
    backups.sort_by(|a, b| b.0.cmp(&a.0));

    if output.json {
        let json_backups: Vec<serde_json::Value> = backups
            .iter()
            .map(|(name, size, modified)| {
                serde_json::json!({
                    "name": name,
                    "size": size,
                    "modified": modified
                })
            })
            .collect();
        output.print_json(&json_backups);
        return Ok(());
    }

    let columns = vec![
        "Backup File".to_string(),
        "Size".to_string(),
        "Modified".to_string(),
    ];

    let rows: Vec<Vec<serde_json::Value>> = backups
        .iter()
        .map(|(name, size, modified)| {
            let size_str = if *size > 1024 * 1024 {
                format!("{:.2} MB", *size as f64 / (1024.0 * 1024.0))
            } else if *size > 1024 {
                format!("{:.2} KB", *size as f64 / 1024.0)
            } else {
                format!("{} B", size)
            };
            vec![
                serde_json::Value::String(name.clone()),
                serde_json::Value::String(size_str),
                serde_json::Value::String(modified.clone()),
            ]
        })
        .collect();

    output.print_table(&columns, &rows);
    output.print_info(&format!("{} backup(s) found", backups.len()));

    Ok(())
}
