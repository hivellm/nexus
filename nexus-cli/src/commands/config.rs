use anyhow::Result;
use clap::{Args, Subcommand};

use super::OutputContext;
use crate::config::{Config, Profile};

#[derive(Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommands,
}

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Show current configuration
    Show,
    /// Initialize configuration file
    Init {
        /// Overwrite existing config
        #[arg(short, long)]
        force: bool,
    },
    /// Set a configuration value
    Set {
        /// Configuration key
        key: String,
        /// Configuration value
        value: String,
    },
    /// Get a configuration value
    Get {
        /// Configuration key
        key: String,
    },
    /// Manage connection profiles
    Profile {
        #[command(subcommand)]
        command: ProfileCommands,
    },
    /// Show configuration file path
    Path,
}

#[derive(Subcommand)]
pub enum ProfileCommands {
    /// List all profiles
    List,
    /// Add a new profile
    Add {
        /// Profile name
        name: String,
        /// Server URL
        #[arg(long)]
        url: String,
        /// API key
        #[arg(long)]
        api_key: Option<String>,
        /// Username
        #[arg(long)]
        username: Option<String>,
        /// Password
        #[arg(long)]
        password: Option<String>,
    },
    /// Remove a profile
    Remove {
        /// Profile name
        name: String,
    },
    /// Set default profile
    Default {
        /// Profile name
        name: String,
    },
}

pub async fn execute(args: ConfigArgs, cfg: &Config, output: &OutputContext) -> Result<()> {
    match args.command {
        ConfigCommands::Show => show_config(cfg, output),
        ConfigCommands::Init { force } => init_config(force, output),
        ConfigCommands::Set { key, value } => set_config(&key, &value, output),
        ConfigCommands::Get { key } => get_config(cfg, &key, output),
        ConfigCommands::Profile { command } => profile_command(command, cfg, output),
        ConfigCommands::Path => show_path(output),
    }
}

fn show_config(cfg: &Config, output: &OutputContext) -> Result<()> {
    if output.json {
        output.print_json(cfg);
        return Ok(());
    }

    println!("Current Configuration");
    println!("=====================");
    println!("URL:      {}", cfg.url.as_deref().unwrap_or("(not set)"));
    println!(
        "API Key:  {}",
        cfg.api_key
            .as_ref()
            .map(|k| format!("{}...", &k[..8.min(k.len())]))
            .unwrap_or_else(|| "(not set)".to_string())
    );
    println!(
        "Username: {}",
        cfg.username.as_deref().unwrap_or("(not set)")
    );
    println!(
        "Password: {}",
        if cfg.password.is_some() {
            "********"
        } else {
            "(not set)"
        }
    );
    println!();
    println!("Profiles: {}", cfg.profiles.len());
    for name in cfg.list_profiles() {
        let is_default = cfg.default_profile.as_ref() == Some(name);
        if is_default {
            println!("  * {} (default)", name);
        } else {
            println!("  - {}", name);
        }
    }

    Ok(())
}

fn init_config(force: bool, output: &OutputContext) -> Result<()> {
    let path = Config::default_path();

    if path.exists() && !force {
        output.print_error(&format!(
            "Configuration file already exists at {}. Use --force to overwrite.",
            path.display()
        ));
        return Ok(());
    }

    let default_config = Config {
        url: Some("http://localhost:3000".to_string()),
        api_key: None,
        username: Some("root".to_string()),
        password: None,
        profiles: Default::default(),
        default_profile: None,
    };

    default_config.save(None)?;
    output.print_success(&format!("Configuration file created at {}", path.display()));

    Ok(())
}

fn set_config(key: &str, value: &str, output: &OutputContext) -> Result<()> {
    let mut cfg = Config::load(None)?;

    match key {
        "url" => cfg.url = Some(value.to_string()),
        "api_key" => cfg.api_key = Some(value.to_string()),
        "username" => cfg.username = Some(value.to_string()),
        "password" => cfg.password = Some(value.to_string()),
        "default_profile" => cfg.default_profile = Some(value.to_string()),
        _ => {
            output.print_error(&format!("Unknown configuration key: {}", key));
            return Ok(());
        }
    }

    cfg.save(None)?;
    output.print_success(&format!("Configuration '{}' updated", key));

    Ok(())
}

fn get_config(cfg: &Config, key: &str, output: &OutputContext) -> Result<()> {
    let value = match key {
        "url" => cfg.url.as_deref(),
        "api_key" => cfg.api_key.as_deref(),
        "username" => cfg.username.as_deref(),
        "password" => cfg.password.as_deref(),
        "default_profile" => cfg.default_profile.as_deref(),
        _ => {
            output.print_error(&format!("Unknown configuration key: {}", key));
            return Ok(());
        }
    };

    match value {
        Some(v) => println!("{}", v),
        None => output.print_info(&format!("'{}' is not set", key)),
    }

    Ok(())
}

fn profile_command(command: ProfileCommands, cfg: &Config, output: &OutputContext) -> Result<()> {
    match command {
        ProfileCommands::List => {
            if cfg.profiles.is_empty() {
                output.print_info("No profiles configured");
                return Ok(());
            }

            println!("Profiles:");
            for (name, profile) in &cfg.profiles {
                let is_default = cfg.default_profile.as_ref() == Some(name);
                if is_default {
                    println!("  * {} (default)", name);
                } else {
                    println!("  - {}", name);
                }
                println!("      URL: {}", profile.url);
            }
        }
        ProfileCommands::Add {
            name,
            url,
            api_key,
            username,
            password,
        } => {
            let mut cfg = Config::load(None)?;
            let profile = Profile {
                url,
                api_key,
                username,
                password,
            };
            cfg.set_profile(&name, profile);
            cfg.save(None)?;
            output.print_success(&format!("Profile '{}' added", name));
        }
        ProfileCommands::Remove { name } => {
            let mut cfg = Config::load(None)?;
            if cfg.remove_profile(&name) {
                cfg.save(None)?;
                output.print_success(&format!("Profile '{}' removed", name));
            } else {
                output.print_error(&format!("Profile '{}' not found", name));
            }
        }
        ProfileCommands::Default { name } => {
            let mut cfg = Config::load(None)?;
            if cfg.profiles.contains_key(&name) {
                cfg.default_profile = Some(name.clone());
                cfg.save(None)?;
                output.print_success(&format!("Default profile set to '{}'", name));
            } else {
                output.print_error(&format!("Profile '{}' not found", name));
            }
        }
    }

    Ok(())
}

fn show_path(output: &OutputContext) -> Result<()> {
    let path = Config::default_path();
    println!("{}", path.display());
    if path.exists() {
        output.print_info("File exists");
    } else {
        output.print_info("File does not exist");
    }
    Ok(())
}
