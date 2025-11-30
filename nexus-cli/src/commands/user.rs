use anyhow::Result;
use clap::{Args, Subcommand};
use serde_json::Value;

use super::OutputContext;
use crate::client::NexusClient;

#[derive(Args)]
pub struct UserArgs {
    #[command(subcommand)]
    pub command: UserCommands,
}

#[derive(Subcommand)]
pub enum UserCommands {
    /// List all users
    List,
    /// Create a new user
    Create {
        /// Username
        username: String,
        /// Password
        #[arg(short, long)]
        password: Option<String>,
        /// Roles (comma-separated)
        #[arg(short, long)]
        roles: Option<String>,
    },
    /// Get user information
    Get {
        /// Username
        username: String,
    },
    /// Update a user
    Update {
        /// Username
        username: String,
        /// New password
        #[arg(short, long)]
        password: Option<String>,
        /// Roles (comma-separated)
        #[arg(short, long)]
        roles: Option<String>,
    },
    /// Change password for a user
    Passwd {
        /// Username (defaults to current user)
        username: Option<String>,
    },
    /// Delete a user
    Delete {
        /// Username
        username: String,
        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },
}

pub async fn execute(client: &NexusClient, args: UserArgs, output: &OutputContext) -> Result<()> {
    match args.command {
        UserCommands::List => list_users(client, output).await,
        UserCommands::Create {
            username,
            password,
            roles,
        } => {
            create_user(
                client,
                &username,
                password.as_deref(),
                roles.as_deref(),
                output,
            )
            .await
        }
        UserCommands::Get { username } => get_user(client, &username, output).await,
        UserCommands::Update {
            username,
            password,
            roles,
        } => {
            update_user(
                client,
                &username,
                password.as_deref(),
                roles.as_deref(),
                output,
            )
            .await
        }
        UserCommands::Passwd { username } => {
            change_password(client, username.as_deref(), output).await
        }
        UserCommands::Delete { username, force } => {
            delete_user(client, &username, force, output).await
        }
    }
}

async fn list_users(client: &NexusClient, output: &OutputContext) -> Result<()> {
    let users = client.get_users().await?;

    if output.json {
        output.print_json(&users);
        return Ok(());
    }

    let columns = vec![
        "Username".to_string(),
        "Roles".to_string(),
        "Permissions".to_string(),
        "Active".to_string(),
        "Root".to_string(),
    ];

    let rows: Vec<Vec<Value>> = users
        .iter()
        .map(|u| {
            vec![
                Value::String(u.username.clone()),
                Value::String(u.roles.join(", ")),
                Value::String(u.permissions.join(", ")),
                Value::Bool(u.is_active),
                Value::Bool(u.is_root),
            ]
        })
        .collect();

    output.print_table(&columns, &rows);
    output.print_info(&format!("{} user(s) found", users.len()));

    Ok(())
}

async fn create_user(
    client: &NexusClient,
    username: &str,
    password: Option<&str>,
    roles: Option<&str>,
    output: &OutputContext,
) -> Result<()> {
    let password = if let Some(p) = password {
        p.to_string()
    } else {
        // Prompt for password
        use dialoguer::Password;
        Password::new()
            .with_prompt("Password")
            .with_confirmation("Confirm password", "Passwords do not match")
            .interact()?
    };

    let roles_vec: Vec<String> = roles
        .map(|r| r.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();

    client.create_user(username, &password, &roles_vec).await?;
    output.print_success(&format!("User '{}' created successfully", username));

    Ok(())
}

async fn get_user(client: &NexusClient, username: &str, output: &OutputContext) -> Result<()> {
    let users = client.get_users().await?;
    let user = users.iter().find(|u| u.username == username);

    match user {
        Some(u) => {
            if output.json {
                output.print_json(u);
            } else {
                println!("Username:    {}", u.username);
                println!("Roles:       {}", u.roles.join(", "));
                println!("Permissions: {}", u.permissions.join(", "));
                println!("Active:      {}", u.is_active);
                println!("Root:        {}", u.is_root);
                if let Some(created) = &u.created_at {
                    println!("Created:     {}", created);
                }
            }
        }
        None => {
            output.print_error(&format!("User '{}' not found", username));
        }
    }

    Ok(())
}

async fn update_user(
    client: &NexusClient,
    username: &str,
    password: Option<&str>,
    roles: Option<&str>,
    output: &OutputContext,
) -> Result<()> {
    let mut updated = false;

    // Update password if provided
    if let Some(new_password) = password {
        let cypher = format!(
            "ALTER USER {} SET PASSWORD '{}'",
            username,
            new_password.replace('\'', "''")
        );
        client.query(&cypher, None).await?;
        output.print_info("Password updated");
        updated = true;
    }

    // Update roles if provided
    if let Some(roles_str) = roles {
        let roles_list: Vec<&str> = roles_str.split(',').map(|s| s.trim()).collect();
        for role in &roles_list {
            let cypher = format!("GRANT ROLE {} TO {}", role, username);
            match client.query(&cypher, None).await {
                Ok(_) => output.print_info(&format!("Role '{}' granted", role)),
                Err(e) => output.print_error(&format!("Failed to grant role '{}': {}", role, e)),
            }
        }
        updated = true;
    }

    if updated {
        output.print_success(&format!("User '{}' updated successfully", username));
    } else {
        output.print_info("No changes specified. Use --password or --roles to update.");
    }

    Ok(())
}

async fn change_password(
    client: &NexusClient,
    username: Option<&str>,
    output: &OutputContext,
) -> Result<()> {
    use dialoguer::Password;

    let target_user = username.unwrap_or("current user");

    // Prompt for new password
    let new_password = Password::new()
        .with_prompt("New password")
        .with_confirmation("Confirm new password", "Passwords do not match")
        .interact()?;

    let cypher = if let Some(user) = username {
        format!(
            "ALTER USER {} SET PASSWORD '{}'",
            user,
            new_password.replace('\'', "''")
        )
    } else {
        format!(
            "ALTER CURRENT USER SET PASSWORD '{}'",
            new_password.replace('\'', "''")
        )
    };

    client.query(&cypher, None).await?;
    output.print_success(&format!("Password changed for {}", target_user));

    Ok(())
}

async fn delete_user(
    client: &NexusClient,
    username: &str,
    force: bool,
    output: &OutputContext,
) -> Result<()> {
    if !force {
        use dialoguer::Confirm;
        let confirmed = Confirm::new()
            .with_prompt(format!(
                "Are you sure you want to delete user '{}'?",
                username
            ))
            .default(false)
            .interact()?;

        if !confirmed {
            output.print_info("Operation cancelled");
            return Ok(());
        }
    }

    client.delete_user(username).await?;
    output.print_success(&format!("User '{}' deleted successfully", username));

    Ok(())
}
