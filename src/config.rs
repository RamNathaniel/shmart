use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use directories::BaseDirs;
use serde::{Deserialize, Serialize};

pub const DEFAULT_LOCAL_MODEL: &str = "ibm-granite/granite-4.2-3b-GGUF:Q4_K_M";
pub const DEFAULT_LOCAL_ENDPOINT: &str = "http://127.0.0.1:8080/v1/chat/completions";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub local: LocalConfig,
    #[serde(default)]
    pub cloud: CloudConfig,
    #[serde(default)]
    pub behavior: BehaviorConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalConfig {
    pub endpoint: String,
    pub model: String,
    #[serde(default)]
    pub api_key_env: String,
    #[serde(default = "default_local_timeout")]
    pub timeout_seconds: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CloudConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub provider: CloudProvider,
    pub endpoint: String,
    pub model: String,
    pub api_key_env: String,
    #[serde(default = "default_cloud_timeout")]
    pub timeout_seconds: u64,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CloudProvider {
    #[default]
    OpenAiCompatible,
    Anthropic,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BehaviorConfig {
    #[serde(default = "default_max_steps")]
    pub max_steps: usize,
    #[serde(default = "default_output_limit")]
    pub output_limit_bytes: usize,
    #[serde(default = "default_command_timeout")]
    pub command_timeout_seconds: u64,
    #[serde(default = "enabled")]
    pub auto_execute_read_only: bool,
}

impl Default for LocalConfig {
    fn default() -> Self {
        Self {
            endpoint: DEFAULT_LOCAL_ENDPOINT.to_owned(),
            model: DEFAULT_LOCAL_MODEL.to_owned(),
            api_key_env: String::new(),
            timeout_seconds: default_local_timeout(),
        }
    }
}

impl Default for CloudConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: CloudProvider::OpenAiCompatible,
            endpoint: String::new(),
            model: String::new(),
            api_key_env: String::new(),
            timeout_seconds: default_cloud_timeout(),
        }
    }
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            max_steps: default_max_steps(),
            output_limit_bytes: default_output_limit(),
            command_timeout_seconds: default_command_timeout(),
            auto_execute_read_only: true,
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
        validate_http_endpoint("local.endpoint", &self.local.endpoint)?;
        if self.local.model.trim().is_empty() {
            bail!("local.model cannot be empty");
        }
        if self.behavior.max_steps == 0 || self.behavior.max_steps > 20 {
            bail!("behavior.max_steps must be between 1 and 20");
        }
        if self.behavior.output_limit_bytes < 1024 {
            bail!("behavior.output_limit_bytes must be at least 1024");
        }

        if self.cloud.enabled {
            validate_http_endpoint("cloud.endpoint", &self.cloud.endpoint)?;
            if self.cloud.model.trim().is_empty() {
                bail!("cloud.model cannot be empty when cloud delegation is enabled");
            }
            if self.cloud.api_key_env.trim().is_empty() {
                bail!("cloud.api_key_env cannot be empty when cloud delegation is enabled");
            }
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

pub fn migrate_legacy_config(destination: &Path) -> Result<()> {
    if destination.exists() {
        return Ok(());
    }
    let dirs = BaseDirs::new().context("could not determine the user configuration directory")?;
    let legacy = dirs.config_dir().join("smartsh").join("config.toml");
    if !legacy.is_file() {
        return Ok(());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::copy(&legacy, destination).with_context(|| {
        format!(
            "failed to migrate {} to {}",
            legacy.display(),
            destination.display()
        )
    })?;
    eprintln!(
        "Migrated configuration from {} to {} (the original was retained).",
        legacy.display(),
        destination.display()
    );
    Ok(())
}

fn validate_http_endpoint(name: &str, endpoint: &str) -> Result<()> {
    if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
        bail!("{name} must start with http:// or https://");
    }
    Ok(())
}

fn default_local_timeout() -> u64 {
    45
}
fn default_cloud_timeout() -> u64 {
    120
}
fn default_max_steps() -> usize {
    6
}
fn default_output_limit() -> usize {
    16 * 1024
}
fn default_command_timeout() -> u64 {
    30
}
fn enabled() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_validate() {
        Config::default().validate().unwrap();
    }

    #[test]
    fn enabled_cloud_requires_model() {
        let mut config = Config::default();
        config.cloud.enabled = true;
        config.cloud.endpoint = "https://example.com/v1/chat/completions".into();
        config.cloud.api_key_env = "EXAMPLE_KEY".into();
        assert!(config.validate().is_err());
    }

    #[test]
    fn old_local_config_without_credential_field_still_loads() {
        let local: LocalConfig = toml::from_str(
            r#"
endpoint = "http://127.0.0.1:8080/v1/chat/completions"
model = "local-model"
"#,
        )
        .unwrap();
        assert!(local.api_key_env.is_empty());
    }
}
