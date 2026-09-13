use std::{
    env, fs,
    io::{self, IsTerminal, Write},
    os::unix::fs::PermissionsExt,
    path::Path,
};

use anyhow::{Context, Result, bail};

use crate::{
    api::ModelApi,
    config::Config,
    decision::{self, Decision},
    executor,
    policy::{self, Verdict},
};

pub async fn run(
    config: &Config,
    request: &str,
    dry_run: bool,
    no_cloud: bool,
    verbose: bool,
) -> Result<()> {
    let cwd = env::current_dir().context("could not determine current directory")?;
    if let Some(explicit) = explicit_command(request) {
        match explicit {
            ExplicitCommand::ChangeDirectory(path) => {
                if dry_run {
                    println!("Dry run: would change directory to {}", path.display());
                } else {
                    env::set_current_dir(&path).with_context(|| {
                        format!("could not change directory to {}", path.display())
                    })?;
                }
            }
            ExplicitCommand::Run(decision) => {
                execute_action(config, &decision, &cwd, dry_run, verbose, true).await?;
            }
        }
        return Ok(());
    }

    let api = ModelApi::new()?;
    let mut observations = Vec::new();

    for step in 1..=config.behavior.max_steps {
        let state = render_state(
            request,
            &cwd,
            &observations,
            step,
            config.behavior.max_steps,
        );
        let mut from_cloud = false;
        let local_response = api.local_decision(config, &state).await;
        let mut decision = match local_response {
            Ok(text) => {
                decision::parse(&text).context("local router returned an unusable decision")?
            }
            Err(error) if config.cloud.enabled && !no_cloud => {
                if verbose {
                    eprintln!(
                        "Local router unavailable; using the configured cloud model: {error}"
                    );
                }
                from_cloud = true;
                cloud_decision(&api, config, &state).await?
            }
            Err(error) => return Err(error).context("local Granite router is unavailable"),
        };

        if let Decision::Delegate { task, reason } = &decision {
            let cloud_state =
                format!("{state}\n\nGranite delegation request:\n{task}\nReason: {reason}");
            if !no_cloud && config.cloud.enabled {
                if verbose {
                    eprintln!("Delegating complex reasoning to {}…", config.cloud.model);
                }
                match cloud_decision(&api, config, &cloud_state).await {
                    Ok(cloud) if decision::validate(&cloud, true).is_ok() => {
                        decision = cloud;
                        from_cloud = true;
                    }
                    Ok(_) => {
                        if verbose {
                            eprintln!("Cloud returned an invalid delegation; continuing locally…");
                        }
                        decision = local_heavy_decision(&api, config, &cloud_state).await?;
                        from_cloud = true;
                    }
                    Err(error) => {
                        if verbose {
                            eprintln!(
                                "Cloud delegation unavailable ({error}); continuing locally…"
                            );
                        }
                        decision = local_heavy_decision(&api, config, &cloud_state).await?;
                        from_cloud = true;
                    }
                }
            } else {
                if verbose {
                    eprintln!("Cloud delegation is disabled; continuing with local Granite…");
                }
                decision = local_heavy_decision(&api, config, &cloud_state).await?;
                from_cloud = true;
            }
        }

        decision::validate(&decision, from_cloud)?;
        match &decision {
            Decision::Answer { message } => {
                println!("{message}");
                return Ok(());
            }
            Decision::Clarify { question } => {
                println!("{question}");
                return Ok(());
            }
            Decision::Run { .. } | Decision::Shell { .. } => {
                if let Some(observation) =
                    execute_action(config, &decision, &cwd, dry_run, verbose, false).await?
                {
                    observations.push(observation);
                } else {
                    return Ok(());
                }
            }
            Decision::Delegate { .. } => unreachable!("delegation is resolved above"),
        }
    }

    bail!(
        "shmart reached its {}-step limit",
        config.behavior.max_steps
    )
}

async fn execute_action(
    config: &Config,
    decision: &Decision,
    cwd: &Path,
    dry_run: bool,
    verbose: bool,
    explicit: bool,
) -> Result<Option<String>> {
    let rendered = executor::display(decision);
    let reason = match decision {
        Decision::Run { reason, .. } | Decision::Shell { reason, .. } => reason,
        _ => bail!("attempted to execute a non-command decision"),
    };
    if verbose || !explicit {
        eprintln!("→ {rendered}");
    }
    if verbose {
        eprintln!("  {reason}");
    }

    if dry_run {
        println!("Dry run: command was not executed.");
        return Ok(None);
    }

    match policy::assess(decision) {
        Verdict::Blocked(reason) => {
            eprintln!("Blocked: {reason}");
            return Ok(Some(format!(
                "POLICY BLOCKED `{rendered}`: {reason}. Choose a safer action or explain the limitation."
            )));
        }
        Verdict::NeedsApproval(reason) => {
            if !confirm(&format!("Run `{rendered}`? {reason}"))? {
                return Ok(Some(format!(
                    "USER DECLINED `{rendered}` because it required approval."
                )));
            }
        }
        Verdict::ReadOnly if !config.behavior.auto_execute_read_only => {
            if !confirm(&format!("Run read-only command `{rendered}`?"))? {
                return Ok(Some(format!("USER DECLINED `{rendered}`.")));
            }
        }
        Verdict::ReadOnly => {}
    }

    let result = executor::execute(
        decision,
        cwd,
        config.behavior.command_timeout_seconds,
        config.behavior.output_limit_bytes,
    )
    .await;
    match result {
        Ok(result) => {
            if !result.stdout.is_empty() {
                print!("{}", result.stdout);
                if !result.stdout.ends_with('\n') {
                    println!();
                }
            }
            if !result.stderr.is_empty() {
                eprint!("{}", result.stderr);
                if !result.stderr.ends_with('\n') {
                    eprintln!();
                }
            }
            Ok(Some(format!(
                "COMMAND: {rendered}\nEXIT: {:?}\nSTDOUT:\n{}\nSTDERR:\n{}",
                result.exit_code, result.stdout, result.stderr
            )))
        }
        Err(error) => {
            eprintln!("Command failed: {error}");
            Ok(Some(format!("COMMAND ERROR for `{rendered}`: {error}")))
        }
    }
}

enum ExplicitCommand {
    ChangeDirectory(std::path::PathBuf),
    Run(Decision),
}

fn explicit_command(input: &str) -> Option<ExplicitCommand> {
    if input
        .chars()
        .any(|character| "|&;<>$`\n*?[]{}".contains(character))
    {
        return None;
    }
    let words = shell_words::split(input).ok()?;
    let (program, args) = words.split_first()?;

    if program == "cd" {
        if args.len() > 1 {
            return None;
        }
        let path = args
            .first()
            .map(std::path::PathBuf::from)
            .or_else(|| env::var_os("HOME").map(std::path::PathBuf::from))?;
        return Some(ExplicitCommand::ChangeDirectory(path));
    }

    if !command_exists(program) {
        return None;
    }
    Some(ExplicitCommand::Run(Decision::Run {
        program: program.clone(),
        args: args.to_vec(),
        reason: "Explicit command entered by the user.".into(),
    }))
}

fn command_exists(program: &str) -> bool {
    if program.contains('/') {
        return is_executable(Path::new(program));
    }
    env::var_os("PATH")
        .map(|paths| {
            env::split_paths(&paths).any(|directory| is_executable(&directory.join(program)))
        })
        .unwrap_or(false)
}

fn is_executable(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

async fn cloud_decision(api: &ModelApi, config: &Config, state: &str) -> Result<Decision> {
    let text = api.cloud_decision(config, state).await?;
    match decision::parse(&text) {
        Ok(decision) => Ok(decision),
        Err(_) => Ok(Decision::Answer { message: text }),
    }
}

async fn local_heavy_decision(api: &ModelApi, config: &Config, state: &str) -> Result<Decision> {
    let local_state = format!(
        "{state}\n\nCloud reasoning is unavailable. Solve the delegated task locally and do not delegate again."
    );
    let text = api.local_heavy_decision(config, &local_state).await?;
    let decision =
        decision::parse(&text).context("local reasoning returned an unusable decision")?;
    decision::validate(&decision, true)?;
    Ok(decision)
}

fn render_state(
    request: &str,
    cwd: &Path,
    observations: &[String],
    step: usize,
    max_steps: usize,
) -> String {
    let os = env::consts::OS;
    let shell = env::var("SHELL").unwrap_or_else(|_| "unknown".into());
    let observations = if observations.is_empty() {
        "None yet.".into()
    } else {
        observations
            .iter()
            .enumerate()
            .map(|(index, item)| format!("Observation {}:\n{}", index + 1, item))
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    format!(
        "User goal:\n{request}\n\nTerminal environment:\nOS: {os}\nShell: {shell}\nWorking directory: {}\nStep: {step}/{max_steps}\n\nPrior observations:\n{observations}\n\nDecision JSON schema:\n{}",
        cwd.display(),
        decision::schema()
    )
}

fn confirm(question: &str) -> Result<bool> {
    if !io::stdin().is_terminal() {
        return Ok(false);
    }
    print!("{question} [y/N] ");
    io::stdout().flush().context("failed to flush stdout")?;
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .context("failed to read confirmation")?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_literal_executables_without_shell_syntax() {
        assert!(matches!(
            explicit_command("/bin/ls -la"),
            Some(ExplicitCommand::Run(Decision::Run { program, args, .. }))
                if program == "/bin/ls" && args == ["-la"]
        ));
        assert!(explicit_command("echo hello | wc -c").is_none());
        assert!(explicit_command("show me the files").is_none());
    }

    #[test]
    fn detects_change_directory_builtin() {
        assert!(matches!(
            explicit_command("cd /tmp"),
            Some(ExplicitCommand::ChangeDirectory(path)) if path == Path::new("/tmp")
        ));
    }
}
