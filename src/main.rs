mod api;
mod cli_study;
mod config;
mod menu;
mod setup;
mod shell;
mod suggestion;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};

use crate::config::Config;

#[derive(Debug, Parser)]
#[command(name = "shmart", version, about)]
struct Cli {
    /// Override the normal shmart configuration path.
    #[arg(long, global = true, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Show model request diagnostics.
    #[arg(long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Configure the model that generates command suggestions.
    Setup(SetupArgs),
    /// Check model configuration and credentials.
    Doctor,
    /// Print the active configuration file path.
    ConfigPath,
    /// Install, inspect, or remove native shell integration.
    Shell(ShellArgs),
    /// Print shell integration code for dotfile managers.
    Init(InitArgs),
    /// Study and manage Python argparse CLIs.
    Cli(CliArgs),
    /// Internal command used by the native shell integration.
    #[command(hide = true)]
    Suggest(SuggestArgs),
}

#[derive(Debug, Args)]
struct ShellArgs {
    #[command(subcommand)]
    command: ShellCommand,
}

#[derive(Debug, Subcommand)]
enum ShellCommand {
    /// Install the native Zsh integration and update .zshrc.
    Install(ZshFileArgs),
    /// Show whether the native Zsh integration is installed.
    Status(ZshPathArgs),
    /// Remove shmart's managed block from .zshrc.
    Uninstall(ZshPathArgs),
}

#[derive(Debug, Args)]
struct ZshFileArgs {
    #[command(flatten)]
    target: ZshPathArgs,
    /// Show the files and source block without changing anything.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct ZshPathArgs {
    /// Shell to integrate with. Only Zsh is supported in this release.
    #[arg(value_enum)]
    shell: SupportedShell,
    /// Override the .zshrc path.
    #[arg(long, value_name = "FILE")]
    zshrc: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum SupportedShell {
    Zsh,
}

#[derive(Debug, Args)]
struct InitArgs {
    /// Shell integration to print.
    #[arg(value_enum)]
    shell: SupportedShell,
}

#[derive(Debug, Args)]
struct SuggestArgs {
    #[arg(long, value_enum)]
    trigger: menu::Trigger,
    #[arg(long)]
    command: String,
    #[arg(long)]
    status: Option<i32>,
    #[arg(long, value_name = "FILE")]
    output: PathBuf,
    #[arg(long, action = clap::ArgAction::Append, num_args = 1, allow_hyphen_values = true)]
    argv: Vec<String>,
}

#[derive(Debug, Args)]
struct CliArgs {
    #[command(subcommand)]
    command: CliStudyCommand,
}

#[derive(Debug, Subcommand)]
enum CliStudyCommand {
    /// Study a Python CLI after displaying the consent warning.
    Study(PythonInvocationArgs),
    /// Show cache identity, freshness, source, and schema counts.
    Status(PythonInvocationArgs),
    /// Print the cached normalized schema for inspection.
    Schema(PythonInvocationArgs),
    /// Discard the current schema and ask to study the CLI again.
    Restudy(PythonInvocationArgs),
    /// Remove the schema and suppression decision for one CLI fingerprint.
    Forget(PythonInvocationArgs),
    /// Inspect or clear the Python CLI schema cache.
    Cache(CliCacheArgs),
}

#[derive(Debug, Args)]
struct PythonInvocationArgs {
    /// Python invocation, for example: python3 tool.py --help
    #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
    invocation: Vec<String>,
}

#[derive(Debug, Args)]
struct CliCacheArgs {
    #[command(subcommand)]
    command: CliCacheCommand,
}

#[derive(Debug, Subcommand)]
enum CliCacheCommand {
    /// List cached CLI identities without schema contents or secrets.
    List,
    /// Delete all cached schemas and suppression decisions after confirmation.
    Clear,
}

#[derive(Clone, Debug, ValueEnum)]
enum SetupProvider {
    Deepseek,
    Openai,
    Openrouter,
    Custom,
}

#[derive(Debug, Args)]
struct SetupArgs {
    /// Provider used to generate command suggestions.
    #[arg(long, value_enum)]
    provider: Option<SetupProvider>,

    /// Provider-specific model identifier.
    #[arg(long)]
    model: Option<String>,

    /// Override the provider API endpoint.
    #[arg(long)]
    endpoint: Option<String>,

    /// Name of the environment variable containing the API key.
    #[arg(long)]
    api_key_env: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config_path = config::resolve_path(cli.config.as_deref())?;

    match cli.command {
        Some(Command::Setup(args)) => {
            setup::run(
                &config_path,
                setup::SetupOptions {
                    provider: args.provider.map(Into::into),
                    model: args.model,
                    endpoint: args.endpoint,
                    api_key_env: args.api_key_env,
                },
            )?;
        }
        Some(Command::Doctor) => {
            let config = Config::load(&config_path).with_context(|| {
                format!(
                    "could not load {}; run `shmart setup` first",
                    config_path.display()
                )
            })?;
            config.validate()?;
            api::doctor(&config, &config_path);
        }
        Some(Command::ConfigPath) => println!("{}", config_path.display()),
        Some(Command::Shell(args)) => match args.command {
            ShellCommand::Install(args) => {
                shell::install_zsh(args.target.zshrc.as_deref(), args.dry_run)?
            }
            ShellCommand::Status(args) => shell::status_zsh(args.zshrc.as_deref())?,
            ShellCommand::Uninstall(args) => shell::uninstall_zsh(args.zshrc.as_deref())?,
        },
        Some(Command::Init(_args)) => shell::print_zsh_init()?,
        Some(Command::Cli(args)) => match args.command {
            CliStudyCommand::Study(args) => {
                let api_key_env = optional_api_key_env(&config_path);
                cli_study::study(&args.invocation, &api_key_env, cli.verbose, false)?;
            }
            CliStudyCommand::Status(args) => cli_study::status(&args.invocation)?,
            CliStudyCommand::Schema(args) => cli_study::show_schema(&args.invocation)?,
            CliStudyCommand::Restudy(args) => {
                let api_key_env = optional_api_key_env(&config_path);
                cli_study::study(&args.invocation, &api_key_env, cli.verbose, true)?;
            }
            CliStudyCommand::Forget(args) => cli_study::forget(&args.invocation)?,
            CliStudyCommand::Cache(args) => match args.command {
                CliCacheCommand::List => cli_study::cache_list()?,
                CliCacheCommand::Clear => cli_study::cache_clear()?,
            },
        },
        Some(Command::Suggest(args)) => {
            let config = load_config(&config_path)?;
            config.validate()?;
            menu::run(
                &config,
                &args.command,
                args.trigger,
                args.status,
                &args.output,
                cli.verbose,
                &args.argv,
            )
            .await?;
        }
        None => {
            Cli::command().print_help()?;
            println!("\n\nInstall the native Zsh integration with: shmart shell install zsh");
        }
    }

    Ok(())
}

fn load_config(path: &std::path::Path) -> Result<Config> {
    Config::load(path).with_context(|| {
        format!(
            "could not load {}; run `shmart setup` first",
            path.display()
        )
    })
}

fn optional_api_key_env(path: &std::path::Path) -> String {
    Config::load(path)
        .map(|config| config.model.api_key_env)
        .unwrap_or_default()
}

impl From<SetupProvider> for setup::ProviderChoice {
    fn from(value: SetupProvider) -> Self {
        match value {
            SetupProvider::Deepseek => Self::DeepSeek,
            SetupProvider::Openai => Self::OpenAi,
            SetupProvider::Openrouter => Self::OpenRouter,
            SetupProvider::Custom => Self::Custom,
        }
    }
}
