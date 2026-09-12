use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SuggestionMenu {
    #[serde(default)]
    pub summary: String,
    pub suggestions: Vec<Suggestion>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Suggestion {
    pub command: String,
    pub explanation: String,
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
            }
        },
        "required": ["summary", "suggestions"],
        "additionalProperties": false
    })
}

pub fn parse(text: &str) -> Result<SuggestionMenu> {
    let object = extract_json_object(text).context("model response contained no JSON menu")?;
    let menu: SuggestionMenu = serde_json::from_str(object)
        .with_context(|| format!("invalid smartsh suggestion menu: {object}"))?;
    if menu.suggestions.is_empty() || menu.suggestions.len() > 5 {
        bail!("suggestion menu must contain between one and five options");
    }
    if menu
        .suggestions
        .iter()
        .any(|item| item.command.trim().is_empty() || item.explanation.trim().is_empty())
    {
        bail!("suggestion menu contains an empty command or explanation");
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
            r#"menu: {"summary":"Try this","suggestions":[{"command":"ls -la","explanation":"List files"}]}"#,
        )
        .unwrap();
        assert_eq!(menu.suggestions[0].command, "ls -la");
    }

    #[test]
    fn rejects_empty_menu() {
        assert!(parse(r#"{"summary":"none","suggestions":[]}"#).is_err());
    }

    #[test]
    fn schema_leaves_item_count_validation_to_local_parser() {
        let schema = schema().to_string();
        assert!(schema.contains("suggestions"));
        assert!(!schema.contains("minItems"));
        assert!(!schema.contains("maxItems"));
    }
}
