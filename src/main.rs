mod agent;
mod api;
mod config;
mod decision;
mod executor;
mod menu;
mod policy;
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

    /// Show routing choices, command explanations, and fallback diagnostics.
    #[arg(long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Configure the local router and optional cloud reasoning model.
    Setup(SetupArgs),
    /// Ask shmart to complete a terminal task.
    Ask(AskArgs),
    /// Check configuration, credentials, and local model connectivity.
    Doctor,
    /// Print the active configuration file path.
    ConfigPath,
    /// Install, inspect, or remove native shell integration.
    Shell(ShellArgs),
    /// Print shell integration code for dotfile managers.
    Init(InitArgs),
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
}

#[derive(Clone, Debug, ValueEnum)]
enum SetupProvider {
    None,
    Openai,
    Anthropic,
    Openrouter,
    Deepseek,
    Custom,
}

#[derive(Debug, Args)]
struct SetupArgs {
    /// Cloud provider used when the immediate model delegates a complex task.
    #[arg(long, value_enum)]
    provider: Option<SetupProvider>,

    /// Cloud model identifier, for example a provider-specific model name.
    #[arg(long)]
    model: Option<String>,

    /// Override the provider API endpoint.
    #[arg(long)]
    endpoint: Option<String>,

    /// Name of the environment variable containing the cloud API key.
    #[arg(long)]
    api_key_env: Option<String>,

    /// Local OpenAI-compatible chat-completions endpoint.
    #[arg(long)]
    local_endpoint: Option<String>,

    /// Model name sent to the local endpoint.
    #[arg(long)]
    local_model: Option<String>,

    /// Environment variable containing the immediate router's API key.
    #[arg(long)]
    local_api_key_env: Option<String>,
}

#[derive(Debug, Args)]
struct AskArgs {
    /// Describe the terminal result you want.
    #[arg(required = true, num_args = 1..)]
    prompt: Vec<String>,

    /// Show the proposed operation without executing it.
    #[arg(long)]
    dry_run: bool,

    /// Prevent delegation to the configured cloud model for this request.
    #[arg(long)]
    no_cloud: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let using_default_config = cli.config.is_none();
    let config_path = config::resolve_path(cli.config.as_deref())?;

    match cli.command {
        Some(Command::Setup(args)) => {
            migrate_config_if_needed(using_default_config, &config_path)?;
            setup::run(
                &config_path,
                setup::SetupOptions {
                    provider: args.provider.map(Into::into),
                    model: args.model,
                    endpoint: args.endpoint,
                    api_key_env: args.api_key_env,
                    local_endpoint: args.local_endpoint,
                    local_model: args.local_model,
                    local_api_key_env: args.local_api_key_env,
                },
            )?;
        }
        Some(Command::Ask(args)) => {
            migrate_config_if_needed(using_default_config, &config_path)?;
            let config = Config::load(&config_path).with_context(|| {
                format!(
                    "could not load {}; run `shmart setup` first",
                    config_path.display()
                )
            })?;
            config.validate()?;
            let prompt = args.prompt.join(" ");
            agent::run(&config, &prompt, args.dry_run, args.no_cloud, cli.verbose).await?;
        }
        Some(Command::Doctor) => {
            migrate_config_if_needed(using_default_config, &config_path)?;
            let config = Config::load(&config_path).with_context(|| {
                format!(
                    "could not load {}; run `shmart setup` first",
                    config_path.display()
                )
            })?;
            config.validate()?;
            api::doctor(&config, &config_path).await?;
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
        Some(Command::Suggest(args)) => {
            migrate_config_if_needed(using_default_config, &config_path)?;
            let config = load_config(&config_path)?;
            config.validate()?;
            menu::run(
                &config,
                &args.command,
                args.trigger,
                args.status,
                &args.output,
                cli.verbose,
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

fn migrate_config_if_needed(using_default_config: bool, path: &std::path::Path) -> Result<()> {
    if using_default_config {
        config::migrate_legacy_config(path)?;
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

impl From<SetupProvider> for setup::ProviderChoice {
    fn from(value: SetupProvider) -> Self {
        match value {
            SetupProvider::None => Self::None,
            SetupProvider::Openai => Self::OpenAi,
            SetupProvider::Anthropic => Self::Anthropic,
            SetupProvider::Openrouter => Self::OpenRouter,
            SetupProvider::Deepseek => Self::DeepSeek,
            SetupProvider::Custom => Self::Custom,
        }
    }
}
