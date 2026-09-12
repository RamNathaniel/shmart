use std::{env, path::Path, time::Duration};

use anyhow::{Context, Result, bail};
use reqwest::{Client, Response};
use serde_json::{Value, json};

use crate::{
    config::{CloudProvider, Config},
    decision,
};

const LOCAL_SYSTEM: &str = r#"You are smartsh's fast immediate terminal router. You control a terminal through typed actions; you are not a source-code completion model.

Choose exactly one next action:
- run: one executable plus an argument array, for a clear terminal step.
- shell: a shell expression only when pipes or redirection are essential.
- delegate: ask the configured cloud model to do complex reasoning.
- answer: finish with a concise response.
- clarify: ask one necessary question.

Delegate when the task needs substantial reasoning, several uncertain steps, diagnosis after an error, or knowledge not present in the terminal state. Prefer a quick local run for obvious inspection commands. Never use sudo. Never hide a shell inside `sh -c`, `bash -c`, or similar; use the shell action instead. Do not claim a command succeeded until its result appears in the state. Return only one JSON object matching the supplied schema."#;

const CLOUD_SYSTEM: &str = r#"You are the careful reasoning tier for smartsh, a terminal assistant. Given the user's goal and terminal observations, choose exactly one next action as JSON: run, shell, answer, or clarify. Do not delegate again. Prefer portable commands, use one executable with an argument array when possible, and reserve shell for necessary pipelines or redirection. Never use sudo. Do not claim success without command output. Return only one JSON object matching the supplied schema."#;

const LOCAL_HEAVY_SYSTEM: &str = r#"You are smartsh's only available reasoning tier. Work through the terminal task carefully using the supplied observations. Choose exactly one next action as JSON: run, shell, answer, or clarify. Cloud delegation is unavailable, so you must not delegate. Prefer portable commands, use one executable with an argument array when possible, and reserve shell for necessary pipelines or redirection. Never use sudo. Do not claim success without command output. Return only one JSON object matching the supplied schema."#;

const SUGGESTION_SYSTEM: &str = r#"You are smartsh's command-repair assistant. Return a concise menu with two to four useful shell-command suggestions. Correct invalid commands using the supplied error output, or translate an explicit `shmart` request into commands. The response must have exactly this shape: {"summary":"...","suggestions":[{"command":"...","explanation":"..."}]}. Each suggestion must include the exact command and a short explanation. Prefer portable, non-destructive commands. Never include sudo. When the smartsh_suggestions function is supplied, call it exactly once."#;

pub struct ModelApi {
    client: Client,
}

struct OpenAiRequest<'a> {
    endpoint: &'a str,
    model: &'a str,
    api_key: Option<&'a str>,
    system: &'a str,
    user: &'a str,
    timeout: Duration,
    structured: bool,
    decision_schema: Value,
    schema_name: &'a str,
    max_tokens: u64,
}

impl ModelApi {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .user_agent(concat!("smartsh/", env!("CARGO_PKG_VERSION")))
                .build()
                .context("failed to build HTTP client")?,
        })
    }

    pub async fn local_decision(&self, config: &Config, state: &str) -> Result<String> {
        let key = immediate_api_key(config)?;
        self.openai_compatible(OpenAiRequest {
            endpoint: &config.local.endpoint,
            model: &config.local.model,
            api_key: key.as_deref(),
            system: LOCAL_SYSTEM,
            user: state,
            timeout: Duration::from_secs(config.local.timeout_seconds),
            structured: true,
            decision_schema: decision::schema(),
            schema_name: "smartsh_decision",
            max_tokens: 1024,
        })
        .await
    }

    pub async fn local_heavy_decision(&self, config: &Config, state: &str) -> Result<String> {
        let key = immediate_api_key(config)?;
        self.openai_compatible(OpenAiRequest {
            endpoint: &config.local.endpoint,
            model: &config.local.model,
            api_key: key.as_deref(),
            system: LOCAL_HEAVY_SYSTEM,
            user: state,
            timeout: Duration::from_secs(config.local.timeout_seconds),
            structured: true,
            decision_schema: decision::execution_schema(),
            schema_name: "smartsh_decision",
            max_tokens: 2048,
        })
        .await
    }

    pub async fn immediate_suggestions(&self, config: &Config, state: &str) -> Result<String> {
        let key = immediate_api_key(config)?;
        self.openai_compatible(OpenAiRequest {
            endpoint: &config.local.endpoint,
            model: &config.local.model,
            api_key: key.as_deref(),
            system: SUGGESTION_SYSTEM,
            user: state,
            timeout: Duration::from_secs(config.local.timeout_seconds),
            structured: true,
            decision_schema: crate::suggestion::schema(),
            schema_name: "smartsh_suggestions",
            max_tokens: 1536,
        })
        .await
    }

    pub async fn cloud_decision(&self, config: &Config, state: &str) -> Result<String> {
        let key = env::var(&config.cloud.api_key_env).with_context(|| {
            format!(
                "cloud API key environment variable {} is not set",
                config.cloud.api_key_env
            )
        })?;
        let timeout = Duration::from_secs(config.cloud.timeout_seconds);
        match config.cloud.provider {
            CloudProvider::OpenAiCompatible => {
                self.openai_compatible(OpenAiRequest {
                    endpoint: &config.cloud.endpoint,
                    model: &config.cloud.model,
                    api_key: Some(&key),
                    system: CLOUD_SYSTEM,
                    user: state,
                    timeout,
                    structured: true,
                    decision_schema: decision::execution_schema(),
                    schema_name: "smartsh_decision",
                    max_tokens: 1024,
                })
                .await
            }
            CloudProvider::Anthropic => {
                self.anthropic(
                    &config.cloud.endpoint,
                    &config.cloud.model,
                    &key,
                    CLOUD_SYSTEM,
                    state,
                    timeout,
                )
                .await
            }
        }
    }

    async fn openai_compatible(&self, request: OpenAiRequest<'_>) -> Result<String> {
        let OpenAiRequest {
            endpoint,
            model,
            api_key,
            system,
            user,
            timeout,
            structured,
            decision_schema,
            schema_name,
            max_tokens,
        } = request;
        let original_endpoint = endpoint;
        let mut request_endpoint = if structured && is_official_deepseek(endpoint) {
            deepseek_beta_endpoint(endpoint)
        } else {
            endpoint.to_owned()
        };
        let mut body = json!({
            "model": model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user}
            ],
            "temperature": 1.0,
            "top_p": 0.95,
            "max_tokens": max_tokens
        });
        if structured {
            body["tools"] = json!([
                {
                    "type": "function",
                    "function": {
                        "name": schema_name,
                        "description": "Return the structured smartsh response.",
                        "parameters": decision_schema,
                        "strict": true
                    }
                }
            ]);
            body["tool_choice"] = json!({
                "type": "function",
                "function": {"name": schema_name}
            });
            body["parallel_tool_calls"] = json!(false);
            // DeepSeek does not allow a named tool choice while thinking mode is
            // enabled. Disable it so the forced strict function call is honored.
            if is_official_deepseek(endpoint) {
                body["thinking"] = json!({"type": "disabled"});
            }
        }

        let send = |endpoint: &str, body: &Value| {
            let mut request = self.client.post(endpoint).timeout(timeout).json(body);
            if let Some(key) = api_key {
                request = request.bearer_auth(key);
            }
            request.send()
        };

        let mut response = send(&request_endpoint, &body)
            .await
            .context("model request failed")?;
        if structured && response.status().is_client_error() {
            // Some OpenAI-compatible servers implement chat but not strict tools.
            let object = body.as_object_mut().expect("JSON body is an object");
            object.remove("tools");
            object.remove("tool_choice");
            object.remove("parallel_tool_calls");
            if structured {
                object.insert("response_format".into(), json!({"type": "json_object"}));
            }
            if request_endpoint != original_endpoint {
                request_endpoint = original_endpoint.to_owned();
            }
            response = send(&request_endpoint, &body)
                .await
                .context("model retry without structured output failed")?;
        }
        let value = response_json(response).await?;
        openai_content(&value).context("model response did not include message content")
    }

    async fn anthropic(
        &self,
        endpoint: &str,
        model: &str,
        key: &str,
        system: &str,
        user: &str,
        timeout: Duration,
    ) -> Result<String> {
        let body = json!({
            "model": model,
            "system": format!("{system}\n\nDecision JSON schema:\n{}", decision::execution_schema()),
            "messages": [{"role": "user", "content": user}],
            "temperature": 0.2,
            "max_tokens": 1024
        });
        let response = self
            .client
            .post(endpoint)
            .timeout(timeout)
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .context("Anthropic request failed")?;
        let value = response_json(response).await?;
        value["content"]
            .as_array()
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item["type"] == "text")
                    .and_then(|item| item["text"].as_str())
            })
            .map(str::to_owned)
            .context("Anthropic response did not include text content")
    }
}

fn is_official_deepseek(endpoint: &str) -> bool {
    endpoint.to_ascii_lowercase().contains("api.deepseek.com")
}

fn deepseek_beta_endpoint(endpoint: &str) -> String {
    if endpoint.to_ascii_lowercase().contains("/beta/") {
        return endpoint.to_owned();
    }
    if let Some(index) = endpoint.find("/chat/completions") {
        return format!(
            "{}/beta{}",
            endpoint[..index].trim_end_matches('/'),
            &endpoint[index..]
        );
    }
    endpoint.to_owned()
}

fn immediate_api_key(config: &Config) -> Result<Option<String>> {
    if config.local.api_key_env.trim().is_empty() {
        return Ok(None);
    }
    env::var(&config.local.api_key_env)
        .map(Some)
        .with_context(|| {
            format!(
                "immediate-router API key environment variable {} is not set",
                config.local.api_key_env
            )
        })
}

async fn response_json(response: Response) -> Result<Value> {
    let status = response.status();
    let text = response
        .text()
        .await
        .context("failed to read model response")?;
    if !status.is_success() {
        let summary: String = text.chars().take(1000).collect();
        bail!("model server returned {status}: {summary}");
    }
    serde_json::from_str(&text)
        .with_context(|| format!("model server returned invalid JSON: {text}"))
}

fn openai_content(value: &Value) -> Option<String> {
    if let Some(arguments) =
        value["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"].as_str()
    {
        return Some(arguments.to_owned());
    }
    let content = &value["choices"][0]["message"]["content"];
    if let Some(text) = content.as_str() {
        return Some(text.to_owned());
    }
    content.as_array().map(|parts| {
        parts
            .iter()
            .filter_map(|part| part["text"].as_str())
            .collect::<Vec<_>>()
            .join("")
    })
}

pub async fn doctor(config: &Config, config_path: &Path) -> Result<()> {
    println!("config: {}", config_path.display());
    println!("immediate router model: {}", config.local.model);
    println!("immediate router endpoint: {}", config.local.endpoint);

    if !config.local.api_key_env.is_empty() {
        if env::var_os(&config.local.api_key_env).is_some() {
            println!(
                "immediate router credential: {} is set",
                config.local.api_key_env
            );
        } else {
            println!(
                "immediate router credential: {} is NOT set",
                config.local.api_key_env
            );
        }
    } else {
        let health_endpoint = config
            .local
            .endpoint
            .trim_end_matches("/v1/chat/completions")
            .trim_end_matches('/')
            .to_owned()
            + "/health";
        match Client::new()
            .get(&health_endpoint)
            .timeout(Duration::from_secs(3))
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => println!("local server: ready"),
            Ok(response) => println!("local server: responded with {}", response.status()),
            Err(error) => println!("local server: unavailable ({error})"),
        }
    }

    if config.cloud.enabled {
        println!("cloud model: {}", config.cloud.model);
        println!("cloud endpoint: {}", config.cloud.endpoint);
        if env::var_os(&config.cloud.api_key_env).is_some() {
            println!("cloud credential: {} is set", config.cloud.api_key_env);
        } else {
            println!("cloud credential: {} is NOT set", config.cloud.api_key_env);
        }
    } else {
        println!("cloud delegation: disabled");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_forced_tool_arguments_before_message_content() {
        let value = json!({
            "choices": [{
                "message": {
                    "content": "ignored",
                    "tool_calls": [{
                        "function": {
                            "name": "smartsh_suggestions",
                            "arguments": "{\"summary\":\"ok\",\"suggestions\":[]}"
                        }
                    }]
                }
            }]
        });
        assert_eq!(
            openai_content(&value).as_deref(),
            Some(r#"{"summary":"ok","suggestions":[]}"#)
        );
    }

    #[test]
    fn extracts_plain_message_content_when_no_tool_call_exists() {
        let value = json!({
            "choices": [{"message": {"content": "{\"kind\":\"answer\"}"}}]
        });
        assert_eq!(
            openai_content(&value).as_deref(),
            Some("{\"kind\":\"answer\"}")
        );
    }

    #[test]
    fn identifies_official_deepseek_endpoints() {
        assert!(is_official_deepseek(
            "https://api.deepseek.com/chat/completions",
        ));
        assert!(!is_official_deepseek(
            "http://localhost:8080/v1/chat/completions"
        ));
    }

    #[test]
    fn maps_deepseek_chat_endpoint_to_beta_for_strict_tools() {
        assert_eq!(
            deepseek_beta_endpoint("https://api.deepseek.com/chat/completions"),
            "https://api.deepseek.com/beta/chat/completions"
        );
        assert_eq!(
            deepseek_beta_endpoint("https://api.deepseek.com/beta/chat/completions"),
            "https://api.deepseek.com/beta/chat/completions"
        );
    }
}
