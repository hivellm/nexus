pub mod admin;
pub mod completion;
pub mod config;
pub mod data;
pub mod db;
pub mod key;
pub mod query;
pub mod schema;
pub mod user;

use comfy_table::{Table, presets::UTF8_FULL};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct OutputContext {
    pub json: bool,
    pub csv: bool,
    pub verbose: bool,
    /// Reserved for future debug output wiring (set from the --debug CLI flag);
    /// read sites will grow in follow-up CLI work.
    #[allow(dead_code)]
    pub debug: bool,
}

/// Creates a spinner with the given message
pub fn create_spinner(message: &str) -> ProgressBar {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
            .template("{spinner:.cyan} {msg}")
            // Invariant: the template is a compile-time string literal
            // with no dynamic placeholders; `indicatif` only errors here
            // on malformed template syntax, which the source proves is
            // impossible.
            .expect("spinner template is a valid compile-time literal"),
    );
    spinner.set_message(message.to_string());
    spinner.enable_steady_tick(Duration::from_millis(100));
    spinner
}

/// Creates a progress bar with the given length. Exposed for future
/// bulk-ingest wiring; callers will re-enable once the long-operation
/// commands land.
#[allow(dead_code)]
pub fn create_progress_bar(len: u64, message: &str) -> ProgressBar {
    let pb = ProgressBar::new(len);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{msg}\n{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
            // See `create_spinner` — compile-time literal template.
            .expect("progress-bar template is a valid compile-time literal")
            .progress_chars("█▓▒░"),
    );
    pb.set_message(message.to_string());
    pb
}

/// Creates a progress bar for download/upload operations. Reserved for
/// future `ingest`/`export` subcommands.
#[allow(dead_code)]
pub fn create_bytes_progress_bar(len: u64, message: &str) -> ProgressBar {
    let pb = ProgressBar::new(len);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{msg}\n{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
            // See `create_spinner` — compile-time literal template.
            .expect("bytes progress-bar template is a valid compile-time literal")
            .progress_chars("█▓▒░"),
    );
    pb.set_message(message.to_string());
    pb
}

/// Creates a multi-progress bar container. Reserved for future parallel
/// ingest workflows.
#[allow(dead_code)]
pub fn create_multi_progress() -> MultiProgress {
    MultiProgress::new()
}

impl OutputContext {
    pub fn print_table(&self, columns: &[String], rows: &[Vec<Value>]) {
        if self.json {
            let result = serde_json::json!({
                "columns": columns,
                "rows": rows,
            });
            // `serde_json::to_string_pretty` on a `serde_json::Value`
            // only fails for custom Serialize impls, and Value's
            // built-in impl is infallible. If that ever changes we'd
            // rather log and continue than panic the CLI.
            match serde_json::to_string_pretty(&result) {
                Ok(s) => println!("{}", s),
                Err(e) => eprintln!("failed to serialize result as JSON: {}", e),
            }
            return;
        }

        if self.csv {
            println!("{}", columns.join(","));
            for row in rows {
                let values: Vec<String> = row.iter().map(value_to_string).collect();
                println!("{}", values.join(","));
            }
            return;
        }

        // Default: table format
        let mut table = Table::new();
        table.load_preset(UTF8_FULL);
        table.set_header(columns);

        for row in rows {
            let values: Vec<String> = row.iter().map(value_to_string).collect();
            table.add_row(values);
        }

        println!("{table}");
    }

    pub fn print_json<T: serde::Serialize>(&self, data: &T) {
        // Table/CSV formats would be nonsensical for free-form JSON
        // responses, so both branches collapse to the same output today.
        // Serialization can fail on custom Serialize impls that
        // themselves return an error; log and continue rather than
        // panic the CLI.
        match serde_json::to_string_pretty(data) {
            Ok(s) => println!("{}", s),
            Err(e) => eprintln!("failed to serialize response as JSON: {}", e),
        }
    }

    pub fn print_success(&self, message: &str) {
        use colored::Colorize;
        println!("{} {}", "✓".green(), message);
    }

    pub fn print_error(&self, message: &str) {
        use colored::Colorize;
        eprintln!("{} {}", "✗".red(), message);
    }

    pub fn print_info(&self, message: &str) {
        use colored::Colorize;
        println!("{} {}", "ℹ".blue(), message);
    }
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(value_to_string).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Object(obj) => serde_json::to_string(obj).unwrap_or_default(),
    }
}
