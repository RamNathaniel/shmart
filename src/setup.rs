use std::io::{self, IsTerminal, Write};
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::config::{CloudProvider, Config};

#[derive(Clone, Copy, Debug)]
pub enum ProviderChoice {
    None,
    OpenAi,
    Anthropic,
    OpenRouter,
    DeepSeek,
    Custom,
}

#[derive(Debug)]
pub struct SetupOptions {
    pub provider: Option<ProviderChoice>,
    pub model: Option<String>,
    pub endpoint: Option<String>,
    pub api_key_env: Option<String>,
    pub local_endpoint: Option<String>,
    pub local_model: Option<String>,
    pub local_api_key_env: Option<String>,
}

pub fn run(path: &Path, options: SetupOptions) -> Result<()> {
    let interactive = io::stdin().is_terminal();
    let mut config = if path.exists() {
        Config::load(path)?
    } else {
        Config::default()
    };

    config.local.endpoint = options.local_endpoint.unwrap_or_else(|| {
        if interactive {
            prompt("Local model endpoint", &config.local.endpoint)
                .unwrap_or_else(|_| config.local.endpoint.clone())
        } else {
            config.local.endpoint.clone()
        }
    });
    config.local.model = options.local_model.unwrap_or_else(|| {
        if interactive {
            prompt("Local model name", &config.local.model)
                .unwrap_or_else(|_| config.local.model.clone())
        } else {
            config.local.model.clone()
        }
    });
    config.local.api_key_env = options
        .local_api_key_env
        .unwrap_or(config.local.api_key_env);

    let choice = match options.provider {
        Some(choice) => choice,
        None if interactive => choose_provider()?,
        None => ProviderChoice::None,
    };

    match choice {
        ProviderChoice::None => config.cloud = Default::default(),
        provider => {
            let preset = provider_preset(provider);
            config.cloud.enabled = true;
            config.cloud.provider = preset.provider;
            config.cloud.endpoint = options.endpoint.unwrap_or_else(|| {
                if interactive {
                    prompt("Cloud API endpoint", preset.endpoint)
                        .unwrap_or_else(|_| preset.endpoint.into())
                } else {
                    preset.endpoint.into()
                }
            });
            config.cloud.api_key_env = options.api_key_env.unwrap_or_else(|| {
                if interactive {
                    prompt("API key environment variable", preset.api_key_env)
                        .unwrap_or_else(|_| preset.api_key_env.into())
                } else {
                    preset.api_key_env.into()
                }
            });
            config.cloud.model = match (options.model, preset.default_model) {
                (Some(model), _) => model,
                (None, Some(default)) if interactive => prompt("Cloud model identifier", default)?,
                (None, Some(default)) => default.into(),
                (None, None) if interactive => prompt_required("Cloud model identifier")?,
                (None, None) => {
                    bail!("--model is required when configuring a cloud provider non-interactively")
                }
            };
        }
    }

    config.validate()?;
    config.save(path)?;
    println!("Saved {}", path.display());
    println!();
    if config.local.api_key_env.is_empty() {
        println!("Start the local router with:");
        println!(
            "  llama serve -hf {} -c 8192 -ngl 99 --jinja --port 8080",
            config.local.model
        );
    } else {
        println!("Immediate router model: {}", config.local.model);
        println!("Immediate router endpoint: {}", config.local.endpoint);
        println!(
            "Set {} in your environment; shmart never stores the key itself.",
            config.local.api_key_env
        );
    }
    if config.cloud.enabled && config.cloud.api_key_env != config.local.api_key_env {
        println!();
        println!(
            "Set {} in your environment; shmart never stores the key itself.",
            config.cloud.api_key_env
        );
    }
    println!();
    println!("Then run: shmart doctor");
    Ok(())
}

struct ProviderPreset {
    provider: CloudProvider,
    endpoint: &'static str,
    api_key_env: &'static str,
    default_model: Option<&'static str>,
}

fn provider_preset(choice: ProviderChoice) -> ProviderPreset {
    match choice {
        ProviderChoice::OpenAi => ProviderPreset {
            provider: CloudProvider::OpenAiCompatible,
            endpoint: "https://api.openai.com/v1/chat/completions",
            api_key_env: "OPENAI_API_KEY",
            default_model: None,
        },
        ProviderChoice::Anthropic => ProviderPreset {
            provider: CloudProvider::Anthropic,
            endpoint: "https://api.anthropic.com/v1/messages",
            api_key_env: "ANTHROPIC_API_KEY",
            default_model: None,
        },
        ProviderChoice::OpenRouter => ProviderPreset {
            provider: CloudProvider::OpenAiCompatible,
            endpoint: "https://openrouter.ai/api/v1/chat/completions",
            api_key_env: "OPENROUTER_API_KEY",
            default_model: None,
        },
        ProviderChoice::DeepSeek => ProviderPreset {
            provider: CloudProvider::OpenAiCompatible,
            endpoint: "https://api.deepseek.com/chat/completions",
            api_key_env: "DEEPSEEK_API_KEY",
            default_model: Some("deepseek-v4-flash"),
        },
        ProviderChoice::Custom => ProviderPreset {
            provider: CloudProvider::OpenAiCompatible,
            endpoint: "http://127.0.0.1:9000/v1/chat/completions",
            api_key_env: "SHMART_CLOUD_API_KEY",
            default_model: None,
        },
        ProviderChoice::None => unreachable!("disabled cloud has no preset"),
    }
}

fn choose_provider() -> Result<ProviderChoice> {
    println!();
    println!("Cloud model used for heavy reasoning:");
    println!("  1. OpenAI");
    println!("  2. Anthropic");
    println!("  3. OpenRouter");
    println!("  4. DeepSeek");
    println!("  5. Custom OpenAI-compatible endpoint");
    println!("  6. Disabled");
    loop {
        let value = prompt("Choice", "6")?;
        match value.as_str() {
            "1" => return Ok(ProviderChoice::OpenAi),
            "2" => return Ok(ProviderChoice::Anthropic),
            "3" => return Ok(ProviderChoice::OpenRouter),
            "4" => return Ok(ProviderChoice::DeepSeek),
            "5" => return Ok(ProviderChoice::Custom),
            "6" => return Ok(ProviderChoice::None),
            _ => eprintln!("Enter a number from 1 to 6."),
        }
    }
}

fn prompt(label: &str, default: &str) -> Result<String> {
    print!("{label} [{default}]: ");
    io::stdout().flush().context("failed to flush stdout")?;
    let mut value = String::new();
    io::stdin()
        .read_line(&mut value)
        .context("failed to read input")?;
    let value = value.trim();
    Ok(if value.is_empty() {
        default.into()
    } else {
        value.into()
    })
}

fn prompt_required(label: &str) -> Result<String> {
    loop {
        print!("{label}: ");
        io::stdout().flush().context("failed to flush stdout")?;
        let mut value = String::new();
        io::stdin()
            .read_line(&mut value)
            .context("failed to read input")?;
        let value = value.trim();
        if !value.is_empty() {
            return Ok(value.into());
        }
        eprintln!("A value is required.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deepseek_preset_uses_official_openai_compatible_endpoint() {
        let preset = provider_preset(ProviderChoice::DeepSeek);
        assert_eq!(preset.provider, CloudProvider::OpenAiCompatible);
        assert_eq!(preset.endpoint, "https://api.deepseek.com/chat/completions");
        assert_eq!(preset.api_key_env, "DEEPSEEK_API_KEY");
        assert_eq!(preset.default_model, Some("deepseek-v4-flash"));
    }
}
