use std::{path::Path, process::Stdio, time::Duration};

use anyhow::{Context, Result, bail};
use tokio::{process::Command, time::timeout};

use crate::decision::Decision;

#[derive(Debug)]
pub struct CommandResult {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

pub async fn execute(
    decision: &Decision,
    cwd: &Path,
    timeout_seconds: u64,
    output_limit: usize,
) -> Result<CommandResult> {
    let mut command = match decision {
        Decision::Run { program, args, .. } => {
            let mut command = Command::new(program);
            command.args(args);
            command
        }
        Decision::Shell { command, .. } => {
            let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/sh".into());
            let mut process = Command::new(shell);
            process.arg("-c").arg(command);
            process
        }
        _ => bail!("attempted to execute a non-command decision"),
    };

    command
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let output = timeout(Duration::from_secs(timeout_seconds), command.output())
        .await
        .context("command timed out")?
        .context("failed to start command")?;

    Ok(CommandResult {
        exit_code: output.status.code(),
        stdout: truncate_lossy(&output.stdout, output_limit),
        stderr: truncate_lossy(&output.stderr, output_limit),
    })
}

fn truncate_lossy(bytes: &[u8], limit: usize) -> String {
    if bytes.len() <= limit {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    let mut text = String::from_utf8_lossy(&bytes[..limit]).into_owned();
    text.push_str("\n[output truncated by shmart]");
    text
}

pub fn display(decision: &Decision) -> String {
    match decision {
        Decision::Run { program, args, .. } => std::iter::once(program.as_str())
            .chain(args.iter().map(String::as_str))
            .map(shell_quote)
            .collect::<Vec<_>>()
            .join(" "),
        Decision::Shell { command, .. } => command.clone(),
        _ => String::new(),
    }
}

fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_+-./:=,@%".contains(&byte))
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}
