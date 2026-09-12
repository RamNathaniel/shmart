use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Decision {
    Run {
        program: String,
        #[serde(default)]
        args: Vec<String>,
        reason: String,
    },
    Shell {
        command: String,
        reason: String,
    },
    Delegate {
        task: String,
        reason: String,
    },
    Answer {
        message: String,
    },
    Clarify {
        question: String,
    },
}

pub fn schema() -> Value {
    schema_with_delegation(true)
}

pub fn execution_schema() -> Value {
    schema_with_delegation(false)
}

fn schema_with_delegation(include_delegation: bool) -> Value {
    let mut variants = vec![
        json!({
            "properties": {
                "kind": {"enum": ["run"]},
                "program": {"type": "string"},
                "args": {"type": "array", "items": {"type": "string"}},
                "reason": {"type": "string"}
            },
            "required": ["kind", "program", "args", "reason"],
            "additionalProperties": false
        }),
        json!({
            "properties": {
                "kind": {"enum": ["shell"]},
                "command": {"type": "string"},
                "reason": {"type": "string"}
            },
            "required": ["kind", "command", "reason"],
            "additionalProperties": false
        }),
    ];
    if include_delegation {
        variants.push(json!({
            "properties": {
                "kind": {"enum": ["delegate"]},
                "task": {"type": "string"},
                "reason": {"type": "string"}
            },
            "required": ["kind", "task", "reason"],
            "additionalProperties": false
        }));
    }
    variants.extend([
        json!({
            "properties": {
                "kind": {"enum": ["answer"]},
                "message": {"type": "string"}
            },
            "required": ["kind", "message"],
            "additionalProperties": false
        }),
        json!({
            "properties": {
                "kind": {"enum": ["clarify"]},
                "question": {"type": "string"}
            },
            "required": ["kind", "question"],
            "additionalProperties": false
        }),
    ]);
    // Keep the union at the root so every object variant can independently
    // declare `additionalProperties: false`, as required by DeepSeek strict
    // tool schemas.
    json!({"anyOf": variants})
}

pub fn parse(text: &str) -> Result<Decision> {
    let object = extract_json_object(text).context("model response contained no JSON object")?;
    serde_json::from_str(object).with_context(|| format!("invalid smartsh decision: {object}"))
}

fn extract_json_object(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let start = bytes.iter().position(|byte| *byte == b'{')?;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (offset, byte) in bytes[start..].iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                in_string = false;
            }
            continue;
        }

        match byte {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..start + offset + 1]);
                }
            }
            _ => {}
        }
    }
    None
}

pub fn validate(decision: &Decision, cloud_response: bool) -> Result<()> {
    match decision {
        Decision::Run { program, .. } if program.trim().is_empty() => bail!("program is empty"),
        Decision::Shell { command, .. } if command.trim().is_empty() => {
            bail!("shell command is empty")
        }
        Decision::Delegate { .. } if cloud_response => {
            bail!("the cloud model attempted to delegate again")
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_fenced_json() {
        let decision =
            parse("result:\n```json\n{\"kind\":\"answer\",\"message\":\"a } b\"}\n```").unwrap();
        assert_eq!(
            decision,
            Decision::Answer {
                message: "a } b".into()
            }
        );
    }

    #[test]
    fn cloud_cannot_redelegate() {
        let decision = Decision::Delegate {
            task: "x".into(),
            reason: "y".into(),
        };
        assert!(validate(&decision, true).is_err());
    }

    #[test]
    fn execution_schema_omits_delegation() {
        let schema = execution_schema().to_string();
        assert!(!schema.contains("delegate"));
        assert!(schema.contains("run"));
        assert!(schema.contains("anyOf"));
        assert!(!schema.contains("oneOf"));
    }
}
