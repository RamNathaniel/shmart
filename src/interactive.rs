use std::{env, path::PathBuf};

use anyhow::{Context, Result};
use reedline::{DefaultPrompt, DefaultPromptSegment, Reedline, Signal};

use crate::{
    api::ModelApi,
    config::Config,
    decision::Decision,
    executor::{self, CommandResult},
    suggestion::{self, SuggestionMenu},
};

pub async fn run(config: &Config, verbose: bool) -> Result<()> {
    let mut editor = Reedline::create();
    let prompt = DefaultPrompt::new(
        DefaultPromptSegment::Basic("smartsh".into()),
        DefaultPromptSegment::Empty,
    );

    println!("smartsh interactive session");
    println!("Commands run normally. Prefix a request with `shmart` for assistance.");
    println!("Type `help` for session commands. Ctrl-D exits.\n");

    loop {
        match editor
            .read_line(&prompt)
            .context("interactive editor failed")?
        {
            Signal::Success(line) => match classify(&line) {
                Input::Empty => {}
                Input::Exit => break,
                Input::Help => print_help(),
                Input::Request(request) => {
                    if let Err(error) = handle_input(&mut editor, config, request, verbose).await {
                        eprintln!("Error: {error:#}");
                    }
                }
            },
            Signal::CtrlC => println!("^C"),
            Signal::CtrlD => break,
            _ => {}
        }
    }

    println!("Goodbye.");
    Ok(())
}

async fn handle_input(
    editor: &mut Reedline,
    config: &Config,
    input: &str,
    verbose: bool,
) -> Result<()> {
    if let Some(intent) = assist_intent(input) {
        if intent.is_empty() {
            println!("Usage: shmart <what you want the shell to do>");
            return Ok(());
        }
        show_suggestions(editor, config, suggestion_context(intent, None), verbose).await?;
        return Ok(());
    }

    if change_directory(input)? {
        return Ok(());
    }

    let result = execute_literal(config, input).await?;
    if is_usage_error(&result) {
        show_suggestions(
            editor,
            config,
            suggestion_context(input, Some(&result)),
            verbose,
        )
        .await?;
    }
    Ok(())
}

async fn show_suggestions(
    editor: &mut Reedline,
    config: &Config,
    context: String,
    verbose: bool,
) -> Result<()> {
    let api = ModelApi::new()?;
    let mut context = context;
    loop {
        if verbose {
            eprintln!(
                "Requesting command suggestions from {}…",
                config.local.model
            );
        }
        let response = api.immediate_suggestions(config, &context).await?;
        let menu = suggestion::parse(&response)?;
        render_menu(&menu);

        let selection_prompt = DefaultPrompt::new(
            DefaultPromptSegment::Basic(format!("choose [1-{}]", menu.suggestions.len())),
            DefaultPromptSegment::Empty,
        );
        let selection = match editor
            .read_line(&selection_prompt)
            .context("suggestion menu failed")?
        {
            Signal::Success(value) => select_menu(&menu, &value),
            Signal::CtrlC | Signal::CtrlD => MenuSelection::Dismiss,
            _ => MenuSelection::Dismiss,
        };

        match selection {
            MenuSelection::Quit | MenuSelection::Dismiss => return Ok(()),
            MenuSelection::Other => {
                let other_prompt = DefaultPrompt::new(
                    DefaultPromptSegment::Basic("describe another way".into()),
                    DefaultPromptSegment::Empty,
                );
                let alternative = match editor
                    .read_line(&other_prompt)
                    .context("other-option prompt failed")?
                {
                    Signal::Success(value) if !value.trim().is_empty() => value,
                    _ => return Ok(()),
                };
                context = format!(
                    "Trigger: follow-up suggestion request\nPrevious context:\n{context}\nUser's alternative description:\n{}",
                    alternative.trim()
                );
            }
            MenuSelection::Command(command) => {
                if change_directory(&command)? {
                    return Ok(());
                }
                execute_literal(config, &command).await?;
                return Ok(());
            }
        }
    }
}

async fn execute_literal(config: &Config, command: &str) -> Result<CommandResult> {
    let cwd = env::current_dir().context("could not determine current directory")?;
    let decision = Decision::Shell {
        command: command.into(),
        reason: "User-entered shell command".into(),
    };
    let result = executor::execute(
        &decision,
        &cwd,
        config.behavior.command_timeout_seconds,
        config.behavior.output_limit_bytes,
    )
    .await?;
    if !result.stdout.is_empty() {
        print_output(&result.stdout, false);
    }
    if !result.stderr.is_empty() {
        print_output(&result.stderr, true);
    }
    Ok(result)
}

fn print_output(output: &str, stderr: bool) {
    if stderr {
        eprint!("{output}");
        if !output.ends_with('\n') {
            eprintln!();
        }
    } else {
        print!("{output}");
        if !output.ends_with('\n') {
            println!();
        }
    }
}

fn assist_intent(input: &str) -> Option<&str> {
    let input = input.trim();
    if input == "shmart" {
        Some("")
    } else {
        input.strip_prefix("shmart ").map(str::trim)
    }
}

fn change_directory(input: &str) -> Result<bool> {
    let words = match shell_words::split(input) {
        Ok(words) => words,
        Err(_) => return Ok(false),
    };
    if words.first().map(String::as_str) != Some("cd") || words.len() > 2 {
        return Ok(false);
    }
    let destination = words
        .get(1)
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(PathBuf::from))
        .context("could not determine the home directory")?;
    env::set_current_dir(&destination)
        .with_context(|| format!("could not change directory to {}", destination.display()))?;
    Ok(true)
}

fn is_usage_error(result: &CommandResult) -> bool {
    let Some(code) = result.exit_code else {
        return false;
    };
    if code == 0 {
        return false;
    }
    let output = format!("{}\n{}", result.stdout, result.stderr).to_ascii_lowercase();
    const MARKERS: &[&str] = &[
        "usage:",
        "no matches found",
        "command not found",
        "is not a git command",
        "not a git command",
        "most similar commands",
        "see 'git --help'",
        "unknown command",
        "unknown option",
        "unrecognized command",
        "unrecognized option",
        "invalid option",
        "illegal option",
        "unexpected argument",
        "requires an argument",
        "required argument",
        "missing operand",
        "too many arguments",
        "parse error",
        "syntax error",
        "did you mean",
    ];
    code == 127 || MARKERS.iter().any(|marker| output.contains(marker))
}

fn suggestion_context(input: &str, result: Option<&CommandResult>) -> String {
    let cwd = env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "unknown".into());
    let platform = std::env::consts::OS;
    match result {
        Some(result) => format!(
            "Trigger: command usage error\nPlatform: {platform}\nWorking directory: {cwd}\nEntered command: {input}\nExit code: {:?}\nStdout:\n{}\nStderr:\n{}",
            result.exit_code, result.stdout, result.stderr
        ),
        None => format!(
            "Trigger: explicit shmart assistance request\nPlatform: {platform}\nWorking directory: {cwd}\nUser intent: {input}"
        ),
    }
}

fn render_menu(menu: &SuggestionMenu) {
    if !menu.summary.is_empty() {
        println!("\n{}", menu.summary);
    }
    for (index, suggestion) in menu.suggestions.iter().enumerate() {
        println!("  {}. {}", index + 1, suggestion.command);
        println!("     {}", suggestion.explanation);
    }
    println!("  o. Other option (describe what you want another way)");
    println!("  q. Quit without running anything");
    println!("\nFYI — optional tools");
    if menu.fyi.is_empty() {
        println!("  No additional tools suggested.");
    } else {
        for tool in &menu.fyi {
            println!("  {} — {}", tool.name, tool.purpose);
            println!("     Install: {}", tool.install);
        }
    }
    println!("\nChoose a number, type another command, o, or q.");
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

#[derive(Debug, PartialEq, Eq)]
enum Input<'a> {
    Empty,
    Exit,
    Help,
    Request(&'a str),
}

fn classify(line: &str) -> Input<'_> {
    let line = line.trim();
    match line {
        "" => Input::Empty,
        "exit" | "quit" | ":q" => Input::Exit,
        "help" | ":help" | "?" => Input::Help,
        request => Input::Request(request),
    }
}

fn print_help() {
    println!("Commands execute literally without contacting an LLM.");
    println!("  shmart <intent>  Ask Smartsh for a menu of command suggestions");
    println!("  help         Show this help");
    println!("  exit, quit   Leave smartsh");
    println!("  Ctrl-C       Cancel the current input");
    println!("  Ctrl-D       Leave smartsh");
    println!("Smartsh also offers suggestions after likely command-usage errors.");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(code: i32, stderr: &str) -> CommandResult {
        CommandResult {
            exit_code: Some(code),
            stdout: String::new(),
            stderr: stderr.into(),
        }
    }

    #[test]
    fn recognizes_session_commands() {
        assert_eq!(classify("  "), Input::Empty);
        assert_eq!(classify("exit"), Input::Exit);
        assert_eq!(classify(":help"), Input::Help);
        assert_eq!(
            classify("show disk usage"),
            Input::Request("show disk usage")
        );
    }

    #[test]
    fn shmart_is_an_explicit_assistance_prefix() {
        assert_eq!(
            assist_intent("shmart list large files"),
            Some("list large files")
        );
        assert_eq!(assist_intent("shmart"), Some(""));
        assert_eq!(assist_intent("sh script.sh"), None);
        assert_eq!(assist_intent("show files"), None);
        assert_eq!(assist_intent("shmartial status"), None);
    }

    #[test]
    fn detects_usage_errors_but_not_general_failures() {
        assert!(is_usage_error(&result(2, "error: unknown option --wat")));
        assert!(is_usage_error(&result(127, "zsh: command not found: wat")));
        assert!(is_usage_error(&result(1, "zsh:1: no matches found: on?")));
        assert!(is_usage_error(&result(
            1,
            "git: 'list' is not a git command. See 'git --help'.\nThe most similar commands are"
        )));
        assert!(!is_usage_error(&result(1, "test suite failed")));
        assert!(!is_usage_error(&result(0, "usage: shown as documentation")));
    }

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
}
