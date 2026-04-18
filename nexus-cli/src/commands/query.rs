use anyhow::Result;
use clap::{Args, Subcommand};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use super::OutputContext;
use crate::client::NexusClient;

/// Get the history file path
fn get_history_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("nexus")
        .join("history.txt")
}

/// Load query history from file
fn load_history() -> Vec<String> {
    let path = get_history_path();
    if path.exists() {
        std::fs::read_to_string(&path)
            .map(|content| content.lines().map(|s| s.to_string()).collect())
            .unwrap_or_default()
    } else {
        Vec::new()
    }
}

/// Save query to history file
fn save_to_history(query: &str) {
    let path = get_history_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // Append to history file (max 1000 entries)
    let mut history = load_history();

    // Don't add duplicates of the last entry
    if history.last().map(|s| s.as_str()) != Some(query) {
        history.push(query.to_string());
    }

    // Keep only last 1000 entries
    if history.len() > 1000 {
        let skip_count = history.len() - 1000;
        history = history.into_iter().skip(skip_count).collect();
    }

    let _ = std::fs::write(&path, history.join("\n"));
}

#[derive(Args)]
pub struct QueryArgs {
    /// Cypher query to execute
    pub query: Option<String>,

    /// Read query from file
    #[arg(short, long)]
    pub file: Option<String>,

    /// Query parameters as JSON
    #[arg(short, long = "params")]
    pub params: Option<String>,

    /// Start interactive query shell (REPL)
    #[arg(short, long)]
    pub interactive: bool,

    /// Execute script file with multiple queries (batch mode)
    #[arg(long)]
    pub batch: Option<String>,

    /// Stop on first error in batch mode
    #[arg(long)]
    pub stop_on_error: bool,

    /// Dry run - parse but don't execute queries
    #[arg(long)]
    pub dry_run: bool,

    /// Show progress in batch mode
    #[arg(long)]
    pub progress: bool,

    /// Limit number of results (pagination)
    #[arg(long)]
    pub limit: Option<usize>,

    /// Skip first N results (pagination)
    #[arg(long)]
    pub skip: Option<usize>,

    /// Filter results by column value (format: column=value)
    #[arg(long = "filter")]
    pub filters: Vec<String>,

    /// Sort results by column (prefix with - for descending)
    #[arg(long)]
    pub sort: Option<String>,

    /// Show query history
    #[arg(long)]
    pub history: bool,
}

pub async fn execute(client: &NexusClient, args: QueryArgs, output: &OutputContext) -> Result<()> {
    // Show history
    if args.history {
        return show_history(output);
    }

    if args.interactive {
        return run_interactive(client, output).await;
    }

    // Batch mode
    if let Some(batch_file) = args.batch {
        return run_batch(
            client,
            &batch_file,
            args.stop_on_error,
            args.dry_run,
            args.progress,
            output,
        )
        .await;
    }

    let query = if let Some(file_path) = args.file {
        std::fs::read_to_string(&file_path)?
    } else if let Some(q) = args.query {
        q
    } else {
        anyhow::bail!(
            "No query provided. Use --interactive for REPL mode or --batch for batch mode."
        );
    };

    // Dry run mode for single query
    if args.dry_run {
        output.print_info(&format!("Would execute: {}", query.trim()));
        return Ok(());
    }

    let params = if let Some(p) = args.params {
        Some(serde_json::from_str(&p)?)
    } else {
        None
    };

    // Save to history
    save_to_history(query.trim());

    let result = client.query(&query, params).await?;

    // Apply filtering, sorting, and pagination
    let mut rows = result.rows.clone();

    // Filter
    for filter in &args.filters {
        if let Some((col, val)) = filter.split_once('=') {
            if let Some(col_idx) = result.columns.iter().position(|c| c == col) {
                rows.retain(|row| {
                    row.get(col_idx)
                        .map(|v| value_matches(v, val))
                        .unwrap_or(false)
                });
            }
        }
    }

    // Sort
    if let Some(sort_col) = &args.sort {
        let (col_name, descending) = if sort_col.starts_with('-') {
            (&sort_col[1..], true)
        } else {
            (sort_col.as_str(), false)
        };

        if let Some(col_idx) = result.columns.iter().position(|c| c == col_name) {
            rows.sort_by(|a, b| {
                let cmp = compare_values(
                    a.get(col_idx).unwrap_or(&serde_json::Value::Null),
                    b.get(col_idx).unwrap_or(&serde_json::Value::Null),
                );
                if descending { cmp.reverse() } else { cmp }
            });
        }
    }

    // Pagination
    let skip = args.skip.unwrap_or(0);
    let total = rows.len();
    rows = rows.into_iter().skip(skip).collect();

    if let Some(limit) = args.limit {
        rows.truncate(limit);
    }

    output.print_table(&result.columns, &rows);

    // Show pagination info if pagination was applied
    if args.skip.is_some() || args.limit.is_some() {
        let shown = rows.len();
        let start = skip + 1;
        let end = skip + shown;
        output.print_info(&format!("Showing {}-{} of {} results", start, end, total));
    }

    if output.verbose {
        if let Some(stats) = result.stats {
            println!("\nStatistics:");
            println!("  Nodes created: {}", stats.nodes_created);
            println!("  Nodes deleted: {}", stats.nodes_deleted);
            println!("  Relationships created: {}", stats.relationships_created);
            println!("  Relationships deleted: {}", stats.relationships_deleted);
            println!("  Properties set: {}", stats.properties_set);
            println!("  Execution time: {:.2}ms", stats.execution_time_ms);
        }
    }

    Ok(())
}

/// Check if a value matches a filter string
fn value_matches(value: &serde_json::Value, filter: &str) -> bool {
    match value {
        serde_json::Value::String(s) => s.to_lowercase().contains(&filter.to_lowercase()),
        serde_json::Value::Number(n) => n.to_string() == filter,
        serde_json::Value::Bool(b) => b.to_string() == filter,
        serde_json::Value::Null => filter.to_lowercase() == "null",
        _ => value
            .to_string()
            .to_lowercase()
            .contains(&filter.to_lowercase()),
    }
}

/// Compare two JSON values for sorting
fn compare_values(a: &serde_json::Value, b: &serde_json::Value) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    match (a, b) {
        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
            let a_f = a.as_f64().unwrap_or(0.0);
            let b_f = b.as_f64().unwrap_or(0.0);
            a_f.partial_cmp(&b_f).unwrap_or(Ordering::Equal)
        }
        (serde_json::Value::String(a), serde_json::Value::String(b)) => a.cmp(b),
        (serde_json::Value::Bool(a), serde_json::Value::Bool(b)) => a.cmp(b),
        (serde_json::Value::Null, serde_json::Value::Null) => Ordering::Equal,
        (serde_json::Value::Null, _) => Ordering::Less,
        (_, serde_json::Value::Null) => Ordering::Greater,
        _ => a.to_string().cmp(&b.to_string()),
    }
}

/// Show query history
fn show_history(output: &OutputContext) -> Result<()> {
    let history = load_history();

    if history.is_empty() {
        output.print_info("No query history found.");
        return Ok(());
    }

    use colored::Colorize;
    println!("{}", "Query History".cyan().bold());
    println!();

    for (i, query) in history.iter().rev().take(50).enumerate() {
        let num = history.len() - i;
        let preview: String = query.chars().take(80).collect();
        let preview = if query.len() > 80 {
            format!("{}...", preview)
        } else {
            preview
        };
        println!("  {:4}  {}", num.to_string().dimmed(), preview);
    }

    println!();
    output.print_info(&format!("{} queries in history", history.len()));

    Ok(())
}

async fn run_interactive(client: &NexusClient, output: &OutputContext) -> Result<()> {
    use colored::Colorize;
    use rustyline::Editor;
    use rustyline::error::ReadlineError;
    use rustyline::history::FileHistory;

    println!("{}", "Nexus Interactive Query Shell".cyan().bold());
    println!("Type your Cypher queries. Use ; to execute, :quit to exit.");
    println!("Use :history to see past queries, :!N to re-run query N.");
    println!("Press {} for keyword completion.\n", "Tab".yellow().bold());

    // Create editor with helper for tab completion and syntax highlighting
    let history_path = get_history_path();
    let mut rl = Editor::<crate::cypher_helper::CypherHelper, FileHistory>::new()?;
    rl.set_helper(Some(crate::cypher_helper::CypherHelper::new()));

    // Load history
    if history_path.exists() {
        let _ = rl.load_history(&history_path);
    }

    let mut buffer = String::new();

    loop {
        // Set prompt based on buffer state
        let prompt = if buffer.is_empty() {
            "nexus> ".to_string()
        } else {
            "    ...> ".to_string()
        };

        // Update colored prompt in helper
        if let Some(helper) = rl.helper_mut() {
            helper.set_colored_prompt(if buffer.is_empty() {
                "nexus> ".green().bold().to_string()
            } else {
                "    ...> ".dimmed().to_string()
            });
        }

        // Read line with tab completion
        let readline = rl.readline(&prompt);

        match readline {
            Ok(line) => {
                let trimmed = line.trim();

                // Check for commands
                if trimmed == ":quit" || trimmed == ":exit" || trimmed == ":q" {
                    println!("Goodbye!");
                    break;
                }

                if trimmed == ":clear" {
                    buffer.clear();
                    println!("Buffer cleared.");
                    continue;
                }

                if trimmed == ":history" || trimmed == ":h" {
                    let history = load_history();
                    if history.is_empty() {
                        println!("No history.");
                    } else {
                        println!("{}", "Query History".cyan());
                        for (i, q) in history.iter().rev().take(20).enumerate() {
                            let num = history.len() - i;
                            let preview: String = q.chars().take(60).collect();
                            println!("  {:3}  {}", num.to_string().dimmed(), preview);
                        }
                    }
                    continue;
                }

                // Re-run history command :!N
                if trimmed.starts_with(":!") {
                    if let Ok(num) = trimmed[2..].parse::<usize>() {
                        let history = load_history();
                        if let Some(query) = history.get(num.saturating_sub(1)) {
                            println!("{} {}", "Executing:".dimmed(), query);
                            match client.query(query, None).await {
                                Ok(result) => {
                                    output.print_table(&result.columns, &result.rows);
                                    println!();
                                }
                                Err(e) => {
                                    output.print_error(&format!("Error: {}", e));
                                }
                            }
                        } else {
                            output.print_error(&format!("History entry {} not found", num));
                        }
                    }
                    continue;
                }

                if trimmed == ":help" || trimmed == ":?" {
                    println!("Commands:");
                    println!("  :quit, :exit, :q  - Exit the shell");
                    println!("  :clear            - Clear the query buffer");
                    println!("  :history, :h      - Show query history");
                    println!("  :!N               - Re-run query number N from history");
                    println!("  :help, :?         - Show this help");
                    println!("  Tab               - Complete keywords");
                    println!("\nEnd queries with ; to execute them.");
                    continue;
                }

                // Add line to buffer
                buffer.push_str(&line);
                buffer.push('\n');

                // Execute if ends with semicolon
                if buffer.trim().ends_with(';') {
                    let query = buffer.trim().trim_end_matches(';').to_string();
                    buffer.clear();

                    if query.is_empty() {
                        continue;
                    }

                    // Add to rustyline history
                    rl.add_history_entry(&query)?;

                    // Save to persistent history file
                    save_to_history(&query);

                    match client.query(&query, None).await {
                        Ok(result) => {
                            output.print_table(&result.columns, &result.rows);
                            println!();
                        }
                        Err(e) => {
                            output.print_error(&format!("Error: {}", e));
                        }
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
                buffer.clear();
            }
            Err(ReadlineError::Eof) => {
                println!("Goodbye!");
                break;
            }
            Err(err) => {
                output.print_error(&format!("Error: {:?}", err));
                break;
            }
        }
    }

    // Save history
    if let Some(parent) = history_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = rl.save_history(&history_path);

    Ok(())
}

/// Parse a script file into individual queries
fn parse_queries(content: &str) -> Vec<String> {
    let mut queries = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut string_char = '"';

    for c in content.chars() {
        match c {
            '"' | '\'' if !in_string => {
                in_string = true;
                string_char = c;
                current.push(c);
            }
            c if in_string && c == string_char => {
                in_string = false;
                current.push(c);
            }
            ';' if !in_string => {
                let query = current.trim().to_string();
                if !query.is_empty() && !query.starts_with("//") && !query.starts_with("--") {
                    queries.push(query);
                }
                current.clear();
            }
            _ => {
                current.push(c);
            }
        }
    }

    // Handle query without trailing semicolon
    let query = current.trim().to_string();
    if !query.is_empty() && !query.starts_with("//") && !query.starts_with("--") {
        queries.push(query);
    }

    queries
}

/// Execute queries in batch mode
async fn run_batch(
    client: &NexusClient,
    file_path: &str,
    stop_on_error: bool,
    dry_run: bool,
    show_progress: bool,
    output: &OutputContext,
) -> Result<()> {
    use colored::Colorize;

    let content = std::fs::read_to_string(file_path)?;
    let queries = parse_queries(&content);

    if queries.is_empty() {
        output.print_info("No queries found in file.");
        return Ok(());
    }

    println!(
        "{}",
        format!("Batch execution: {} queries", queries.len())
            .cyan()
            .bold()
    );
    println!();

    let mut success_count = 0;
    let mut error_count = 0;
    let mut total_time = 0.0;

    for (i, query) in queries.iter().enumerate() {
        let query_num = i + 1;
        let query_preview: String = query.chars().take(60).collect();
        let query_preview = if query.len() > 60 {
            format!("{}...", query_preview)
        } else {
            query_preview
        };

        if show_progress {
            print!(
                "[{}/{}] {} ",
                query_num,
                queries.len(),
                query_preview.dimmed()
            );
            io::stdout().flush()?;
        }

        if dry_run {
            if show_progress {
                println!("{}", "[DRY RUN]".yellow());
            } else {
                println!(
                    "{}: {}",
                    format!("Query {}", query_num).cyan(),
                    query_preview
                );
            }
            success_count += 1;
            continue;
        }

        match client.query(query, None).await {
            Ok(result) => {
                success_count += 1;
                if let Some(ref stats) = result.stats {
                    total_time += stats.execution_time_ms;
                }

                if show_progress {
                    println!("{}", "OK".green());
                } else {
                    println!(
                        "{}: {}",
                        format!("Query {}", query_num).green(),
                        query_preview
                    );
                    if !result.rows.is_empty() {
                        output.print_table(&result.columns, &result.rows);
                    }
                }
            }
            Err(e) => {
                error_count += 1;

                if show_progress {
                    println!("{}", "FAILED".red());
                }
                output.print_error(&format!("Query {}: {}", query_num, e));

                if stop_on_error {
                    println!();
                    output.print_error("Stopping on error (--stop-on-error)");
                    break;
                }
            }
        }
    }

    // Summary
    println!();
    println!("{}", "Batch Summary".cyan().bold());
    println!("  Total queries: {}", queries.len());
    println!("  Successful:    {}", format!("{}", success_count).green());
    if error_count > 0 {
        println!("  Failed:        {}", format!("{}", error_count).red());
    } else {
        println!("  Failed:        {}", error_count);
    }
    if !dry_run && total_time > 0.0 {
        println!("  Total time:    {:.2}ms", total_time);
    }

    if error_count > 0 {
        anyhow::bail!("{} queries failed", error_count);
    }

    Ok(())
}
