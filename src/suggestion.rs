use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SuggestionMenu {
    #[serde(default)]
    pub summary: String,
    pub suggestions: Vec<Suggestion>,
    #[serde(default)]
    pub fyi: Vec<ToolSuggestion>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Suggestion {
    pub command: String,
    pub explanation: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolSuggestion {
    pub name: String,
    pub purpose: String,
    pub install: String,
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "summary": {"type": "string"},
            "suggestions": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "command": {"type": "string"},
                        "explanation": {"type": "string"}
                    },
                    "required": ["command", "explanation"],
                    "additionalProperties": false
                }
            },
            "fyi": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "purpose": {"type": "string"},
                        "install": {"type": "string"}
                    },
                    "required": ["name", "purpose", "install"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["summary", "suggestions", "fyi"],
        "additionalProperties": false
    })
}

pub fn parse(text: &str) -> Result<SuggestionMenu> {
    let object = extract_json_object(text).context("model response contained no JSON menu")?;
    let menu: SuggestionMenu = serde_json::from_str(object)
        .with_context(|| format!("invalid shmart suggestion menu: {object}"))?;
    if menu.suggestions.is_empty() || menu.suggestions.len() > 5 {
        bail!("suggestion menu must contain between one and five options");
    }
    if menu.fyi.len() > 3 {
        bail!("FYI tool recommendations must contain at most three options");
    }
    if menu
        .suggestions
        .iter()
        .any(|item| item.command.trim().is_empty() || item.explanation.trim().is_empty())
    {
        bail!("suggestion menu contains an empty command or explanation");
    }
    if menu.fyi.iter().any(|tool| {
        tool.name.trim().is_empty()
            || tool.purpose.trim().is_empty()
            || tool.install.trim().is_empty()
    }) {
        bail!("FYI tool recommendations contain an empty field");
    }
    Ok(menu)
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
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(&text[start..start + offset + 1]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_menu_from_surrounding_text() {
        let menu = parse(
            r#"menu: {"summary":"Try this","suggestions":[{"command":"ls -la","explanation":"List files"}],"fyi":[{"name":"ripgrep","purpose":"Fast recursive search","install":"brew install ripgrep"}]}"#,
        )
        .unwrap();
        assert_eq!(menu.suggestions[0].command, "ls -la");
        assert_eq!(menu.fyi[0].name, "ripgrep");
    }

    #[test]
    fn rejects_empty_menu() {
        assert!(parse(r#"{"summary":"none","suggestions":[]}"#).is_err());
    }

    #[test]
    fn schema_leaves_item_count_validation_to_local_parser() {
        let schema = schema().to_string();
        assert!(schema.contains("suggestions"));
        assert!(schema.contains("fyi"));
        assert!(!schema.contains("minItems"));
        assert!(!schema.contains("maxItems"));
    }
}
