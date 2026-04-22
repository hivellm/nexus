use anyhow::Result;
use clap::{Args, CommandFactory, ValueEnum};
use clap_complete::{Shell, generate};
use std::io;

use super::OutputContext;

#[derive(Args)]
pub struct CompletionArgs {
    /// Shell type to generate completion for
    #[arg(value_enum)]
    pub shell: ShellType,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum ShellType {
    /// Bash shell
    Bash,
    /// Zsh shell
    Zsh,
    /// Fish shell
    Fish,
    /// PowerShell
    PowerShell,
    /// Elvish shell
    Elvish,
}

impl From<ShellType> for Shell {
    fn from(shell: ShellType) -> Self {
        match shell {
            ShellType::Bash => Shell::Bash,
            ShellType::Zsh => Shell::Zsh,
            ShellType::Fish => Shell::Fish,
            ShellType::PowerShell => Shell::PowerShell,
            ShellType::Elvish => Shell::Elvish,
        }
    }
}

pub async fn execute(args: CompletionArgs, _output: &OutputContext) -> Result<()> {
    let mut cmd = crate::Cli::command();
    let shell: Shell = args.shell.into();

    generate(shell, &mut cmd, "nexus", &mut io::stdout());

    Ok(())
}
