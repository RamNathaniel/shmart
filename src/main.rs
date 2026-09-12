mod agent;
mod api;
mod config;
mod decision;
mod executor;
mod interactive;
mod policy;
mod setup;
mod suggestion;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::config::Config;

#[derive(Debug, Parser)]
#[command(name = "smartsh", version, about)]
struct Cli {
    /// Override the normal smartsh configuration path.
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
    /// Ask smartsh to complete a terminal task.
    Ask(AskArgs),
    /// Check configuration, credentials, and local model connectivity.
    Doctor,
    /// Print the active configuration file path.
    ConfigPath,
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
    /// Cloud provider used when Granite delegates a complex task.
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
                    local_endpoint: args.local_endpoint,
                    local_model: args.local_model,
                    local_api_key_env: args.local_api_key_env,
                },
            )?;
        }
        Some(Command::Ask(args)) => {
            let config = Config::load(&config_path).with_context(|| {
                format!(
                    "could not load {}; run `smartsh setup` first",
                    config_path.display()
                )
            })?;
            config.validate()?;
            let prompt = args.prompt.join(" ");
            agent::run(&config, &prompt, args.dry_run, args.no_cloud, cli.verbose).await?;
        }
        Some(Command::Doctor) => {
            let config = Config::load(&config_path).with_context(|| {
                format!(
                    "could not load {}; run `smartsh setup` first",
                    config_path.display()
                )
            })?;
            config.validate()?;
            api::doctor(&config, &config_path).await?;
        }
        Some(Command::ConfigPath) => println!("{}", config_path.display()),
        None => {
            let config = load_config(&config_path)?;
            config.validate()?;
            interactive::run(&config, cli.verbose).await?;
        }
    }

    Ok(())
}

fn load_config(path: &std::path::Path) -> Result<Config> {
    Config::load(path).with_context(|| {
        format!(
            "could not load {}; run `smartsh setup` first",
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
