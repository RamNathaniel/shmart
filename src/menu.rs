use std::{
    env, fs,
    io::{self, IsTerminal, Write},
    path::Path,
};

use anyhow::{Context, Result};

use crate::{
    api::ModelApi,
    config::Config,
    suggestion::{self, SuggestionMenu},
};

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum Trigger {
    Explicit,
    Error,
}

pub async fn run(
    config: &Config,
    command: &str,
    trigger: Trigger,
    status: Option<i32>,
    output: &Path,
    verbose: bool,
) -> Result<()> {
    // An empty file means "dismissed" to the Zsh integration.
    fs::write(output, "").with_context(|| format!("could not initialize {}", output.display()))?;

    let mut context = suggestion_context(command, trigger, status);
    loop {
        if verbose {
            eprintln!(
                "Requesting command suggestions from {}…",
                config.local.model
            );
        }
        let response = ModelApi::new()?
            .immediate_suggestions(config, &context)
            .await?;
        let menu = suggestion::parse(&response)?;
        render_menu(&menu);

        let selection = read_line(&format!("choose [1-{}]", menu.suggestions.len()))?
            .map(|value| select_menu(&menu, &value))
            .unwrap_or(MenuSelection::Dismiss);

        match selection {
            MenuSelection::Quit | MenuSelection::Dismiss => return Ok(()),
            MenuSelection::Command(command) => {
                fs::write(output, command)
                    .with_context(|| format!("could not write {}", output.display()))?;
                return Ok(());
            }
            MenuSelection::Other => {
                let alternative = match read_line("describe another way")? {
                    Some(value) if !value.trim().is_empty() => value,
                    _ => return Ok(()),
                };
                context = format!(
                    "Trigger: follow-up suggestion request\nPrevious context:\n{context}\nUser's alternative description:\n{}",
                    alternative.trim()
                );
            }
        }
    }
}

fn read_line(prompt: &str) -> Result<Option<String>> {
    print!("{prompt}> ");
    io::stdout()
        .flush()
        .context("failed to display menu prompt")?;
    let mut value = String::new();
    let bytes = io::stdin()
        .read_line(&mut value)
        .context("failed to read menu selection")?;
    if bytes == 0 {
        println!();
        return Ok(None);
    }
    Ok(Some(value.trim_end_matches(['\r', '\n']).to_owned()))
}

fn suggestion_context(command: &str, trigger: Trigger, status: Option<i32>) -> String {
    let cwd = env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "unknown".into());
    let platform = env::consts::OS;
    match trigger {
        Trigger::Explicit => format!(
            "Trigger: explicit shmart assistance request\nPlatform: {platform}\nShell: zsh\nWorking directory: {cwd}\nUser intent: {command}"
        ),
        Trigger::Error => format!(
            "Trigger: likely command usage error\nPlatform: {platform}\nShell: zsh\nWorking directory: {cwd}\nEntered command: {command}\nExit code: {}\nNote: native shell integration does not capture stderr; infer the likely usage problem from the command and status.",
            status
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".into())
        ),
    }
}

fn render_menu(menu: &SuggestionMenu) {
    let styled = io::stdout().is_terminal() && env::var_os("NO_COLOR").is_none();
    if !menu.summary.is_empty() {
        let summary = display_text(&menu.summary);
        if styled {
            println!("\n\x1b[1m{summary}\x1b[0m");
        } else {
            println!("\n{summary}");
        }
    }
    for (index, suggestion) in menu.suggestions.iter().enumerate() {
        let command = display_text(&suggestion.command);
        let explanation = display_text(&suggestion.explanation);
        if styled {
            println!("  {}. \x1b[1;36m{command}\x1b[0m", index + 1);
            println!("     \x1b[2m{explanation}\x1b[0m");
        } else {
            println!("  {}. {command}", index + 1);
            println!("     {explanation}");
        }
    }
    if styled {
        println!("  \x1b[1;33mo\x1b[0m. Other option (describe what you want another way)");
        println!("  \x1b[1;33mq\x1b[0m. Quit without running anything");
        println!("\n\x1b[1;34mFYI — optional tools\x1b[0m");
    } else {
        println!("  o. Other option (describe what you want another way)");
        println!("  q. Quit without running anything");
        println!("\nFYI — optional tools");
    }
    if menu.fyi.is_empty() {
        if styled {
            println!("  \x1b[2mNo additional tools suggested.\x1b[0m");
        } else {
            println!("  No additional tools suggested.");
        }
    } else {
        for tool in &menu.fyi {
            let name = display_text(&tool.name);
            let purpose = display_text(&tool.purpose);
            let install = display_text(&tool.install);
            if styled {
                println!("  \x1b[1;34m{name}\x1b[0m — {purpose}");
                println!("     Install: \x1b[1;36m{install}\x1b[0m");
            } else {
                println!("  {name} — {purpose}");
                println!("     Install: {install}");
            }
        }
    }
    if styled {
        println!("\n\x1b[1mChoose a number, type another command, o, or q.\x1b[0m");
    } else {
        println!("\nChoose a number, type another command, o, or q.");
    }
}

fn display_text(value: &str) -> String {
    let mut displayed = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_control() {
            displayed.extend(character.escape_default());
        } else {
            displayed.push(character);
        }
    }
    displayed
}

#[derive(Debug, PartialEq, Eq)]
enum MenuSelection {
    Command(String),
    Other,
    Quit,
    Dismiss,
}

fn select_menu(menu: &SuggestionMenu, input: &str) -> MenuSelection {
    let input = input.trim();
    if input.is_empty() {
        return MenuSelection::Dismiss;
    }
    if input.eq_ignore_ascii_case("o") {
        return MenuSelection::Other;
    }
    if input.eq_ignore_ascii_case("q") {
        return MenuSelection::Quit;
    }
    if let Ok(index) = input.parse::<usize>() {
        return index
            .checked_sub(1)
            .and_then(|index| menu.suggestions.get(index))
            .map(|suggestion| MenuSelection::Command(suggestion.command.clone()))
            .unwrap_or(MenuSelection::Dismiss);
    }
    MenuSelection::Command(input.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_selection_accepts_numbers_or_commands() {
        let menu = SuggestionMenu {
            summary: "Try one".into(),
            suggestions: vec![suggestion::Suggestion {
                command: "ls -la".into(),
                explanation: "List files".into(),
            }],
            fyi: Vec::new(),
        };
        assert_eq!(
            select_menu(&menu, "1"),
            MenuSelection::Command("ls -la".into())
        );
        assert_eq!(
            select_menu(&menu, "pwd"),
            MenuSelection::Command("pwd".into())
        );
        assert_eq!(select_menu(&menu, "o"), MenuSelection::Other);
        assert_eq!(select_menu(&menu, "q"), MenuSelection::Quit);
        assert_eq!(select_menu(&menu, ""), MenuSelection::Dismiss);
        assert_eq!(select_menu(&menu, "9"), MenuSelection::Dismiss);
    }

    #[test]
    fn terminal_control_characters_are_escaped_for_display() {
        assert_eq!(
            display_text("git status\n\u{1b}[2J"),
            "git status\\n\\u{1b}[2J"
        );
    }
}
