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
    pub debug: bool,
}

/// Creates a spinner with the given message
pub fn create_spinner(message: &str) -> ProgressBar {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    spinner.set_message(message.to_string());
    spinner.enable_steady_tick(Duration::from_millis(100));
    spinner
}

/// Creates a progress bar with the given length
pub fn create_progress_bar(len: u64, message: &str) -> ProgressBar {
    let pb = ProgressBar::new(len);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{msg}\n{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
            .unwrap()
            .progress_chars("█▓▒░")
    );
    pb.set_message(message.to_string());
    pb
}

/// Creates a progress bar for download/upload operations
pub fn create_bytes_progress_bar(len: u64, message: &str) -> ProgressBar {
    let pb = ProgressBar::new(len);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{msg}\n{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
            .unwrap()
            .progress_chars("█▓▒░")
    );
    pb.set_message(message.to_string());
    pb
}

/// Creates a multi-progress bar container
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
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
            return;
        }

        if self.csv {
            println!("{}", columns.join(","));
            for row in rows {
                let values: Vec<String> = row.iter().map(|v| value_to_string(v)).collect();
                println!("{}", values.join(","));
            }
            return;
        }

        // Default: table format
        let mut table = Table::new();
        table.load_preset(UTF8_FULL);
        table.set_header(columns);

        for row in rows {
            let values: Vec<String> = row.iter().map(|v| value_to_string(v)).collect();
            table.add_row(values);
        }

        println!("{table}");
    }

    pub fn print_json<T: serde::Serialize>(&self, data: &T) {
        if self.json {
            println!("{}", serde_json::to_string_pretty(data).unwrap());
        } else {
            println!("{}", serde_json::to_string_pretty(data).unwrap());
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
            let items: Vec<String> = arr.iter().map(|i| value_to_string(i)).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Object(obj) => serde_json::to_string(obj).unwrap_or_default(),
    }
}
