use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use directories::BaseDirs;
use serde::{Deserialize, Serialize};

pub const DEFAULT_MODEL: &str = "deepseek-v4-flash";
pub const DEFAULT_ENDPOINT: &str = "https://api.deepseek.com/chat/completions";
pub const DEFAULT_API_KEY_ENV: &str = "DEEPSEEK_API_KEY";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub model: ModelConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelConfig {
    pub endpoint: String,
    pub model: String,
    #[serde(default)]
    pub api_key_env: String,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            endpoint: DEFAULT_ENDPOINT.into(),
            model: DEFAULT_MODEL.into(),
            api_key_env: DEFAULT_API_KEY_ENV.into(),
            timeout_seconds: default_timeout(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("invalid TOML in {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let text = toml::to_string_pretty(self).context("failed to serialize configuration")?;
        fs::write(path, text).with_context(|| format!("failed to write {}", path.display()))
    }

    pub fn validate(&self) -> Result<()> {
        if !(self.model.endpoint.starts_with("http://")
            || self.model.endpoint.starts_with("https://"))
        {
            bail!("model.endpoint must start with http:// or https://");
        }
        if self.model.model.trim().is_empty() {
            bail!("model.model cannot be empty");
        }
        if self.model.timeout_seconds == 0 {
            bail!("model.timeout_seconds must be greater than zero");
        }
        Ok(())
    }
}

pub fn resolve_path(override_path: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = override_path {
        return Ok(path.to_path_buf());
    }
    let dirs = BaseDirs::new().context("could not determine the user configuration directory")?;
    Ok(dirs.config_dir().join("shmart").join("config.toml"))
}

fn default_timeout() -> u64 {
    45
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_deepseek_and_validates() {
        let config = Config::default();
        config.validate().unwrap();
        assert_eq!(config.model.model, DEFAULT_MODEL);
        assert_eq!(config.model.endpoint, DEFAULT_ENDPOINT);
    }

    #[test]
    fn rejects_retired_configuration_sections() {
        let result = toml::from_str::<Config>(
            r#"
[local]
endpoint = "http://localhost:8080/v1/chat/completions"
model = "retired"
"#,
        );
        assert!(result.is_err());
    }
}
