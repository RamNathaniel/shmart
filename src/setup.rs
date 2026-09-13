use std::io::{self, IsTerminal, Write};
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::config::Config;

#[derive(Clone, Copy, Debug)]
pub enum ProviderChoice {
    DeepSeek,
    OpenAi,
    OpenRouter,
    Custom,
}

#[derive(Debug)]
pub struct SetupOptions {
    pub provider: Option<ProviderChoice>,
    pub model: Option<String>,
    pub endpoint: Option<String>,
    pub api_key_env: Option<String>,
}

pub fn run(path: &Path, options: SetupOptions) -> Result<()> {
    let interactive = io::stdin().is_terminal();
    let mut config = if path.exists() {
        Config::load(path)?
    } else {
        Config::default()
    };
    let provider = match options.provider {
        Some(provider) => provider,
        None if interactive => choose_provider()?,
        None => ProviderChoice::DeepSeek,
    };
    let preset = provider_preset(provider);

    config.model.endpoint = match options.endpoint {
        Some(endpoint) => endpoint,
        None if matches!(provider, ProviderChoice::Custom) && interactive => {
            prompt("Model API endpoint", &config.model.endpoint)?
        }
        None if matches!(provider, ProviderChoice::Custom) => {
            bail!("--endpoint is required with --provider custom")
        }
        None => preset.endpoint.into(),
    };
    config.model.api_key_env = match options.api_key_env {
        Some(name) => name,
        None if matches!(provider, ProviderChoice::Custom) && interactive => {
            prompt("API key environment variable (empty for none)", "")?
        }
        None => preset.api_key_env.into(),
    };
    config.model.model = match (options.model, preset.default_model) {
        (Some(model), _) => model,
        (None, Some(default)) => default.into(),
        (None, None) if interactive => prompt_required("Model identifier")?,
        (None, None) => bail!("--model is required for this provider"),
    };

    config.validate()?;
    config.save(path)?;
    println!("Saved {}", path.display());
    println!("Model: {}", config.model.model);
    println!("Endpoint: {}", config.model.endpoint);
    if !config.model.api_key_env.is_empty() {
        println!(
            "Set {} in your environment; shmart never stores the key itself.",
            config.model.api_key_env
        );
    }
    println!("Then run: shmart doctor");
    Ok(())
}

struct ProviderPreset {
    endpoint: &'static str,
    api_key_env: &'static str,
    default_model: Option<&'static str>,
}

fn provider_preset(choice: ProviderChoice) -> ProviderPreset {
    match choice {
        ProviderChoice::DeepSeek => ProviderPreset {
            endpoint: "https://api.deepseek.com/chat/completions",
            api_key_env: "DEEPSEEK_API_KEY",
            default_model: Some("deepseek-v4-flash"),
        },
        ProviderChoice::OpenAi => ProviderPreset {
            endpoint: "https://api.openai.com/v1/chat/completions",
            api_key_env: "OPENAI_API_KEY",
            default_model: None,
        },
        ProviderChoice::OpenRouter => ProviderPreset {
            endpoint: "https://openrouter.ai/api/v1/chat/completions",
            api_key_env: "OPENROUTER_API_KEY",
            default_model: None,
        },
        ProviderChoice::Custom => ProviderPreset {
            endpoint: "",
            api_key_env: "",
            default_model: None,
        },
    }
}

fn choose_provider() -> Result<ProviderChoice> {
    println!("\nSuggestion model provider:");
    println!("  1. DeepSeek");
    println!("  2. OpenAI");
    println!("  3. OpenRouter");
    println!("  4. Custom OpenAI-compatible endpoint");
    loop {
        match prompt("Choice", "1")?.as_str() {
            "1" => return Ok(ProviderChoice::DeepSeek),
            "2" => return Ok(ProviderChoice::OpenAi),
            "3" => return Ok(ProviderChoice::OpenRouter),
            "4" => return Ok(ProviderChoice::Custom),
            _ => eprintln!("Enter a number from 1 to 4."),
        }
    }
}

fn prompt(label: &str, default: &str) -> Result<String> {
    if default.is_empty() {
        print!("{label}: ");
    } else {
        print!("{label} [{default}]: ");
    }
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
        let value = prompt(label, "")?;
        if !value.is_empty() {
            return Ok(value);
        }
        eprintln!("A value is required.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deepseek_is_the_complete_default_preset() {
        let preset = provider_preset(ProviderChoice::DeepSeek);
        assert_eq!(preset.endpoint, "https://api.deepseek.com/chat/completions");
        assert_eq!(preset.api_key_env, "DEEPSEEK_API_KEY");
        assert_eq!(preset.default_model, Some("deepseek-v4-flash"));
    }
}
