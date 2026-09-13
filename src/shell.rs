use std::{
    env, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use directories::BaseDirs;

const ZSH_PLUGIN_TEMPLATE: &str = include_str!("../shell/shmart.zsh");
const BASH_PLUGIN_TEMPLATE: &str = include_str!("../shell/shmart.bash");
const BLOCK_BEGIN: &str = "# >>> shmart shell integration >>>";
const BLOCK_END: &str = "# <<< shmart shell integration <<<";

#[derive(Clone, Copy)]
struct ShellSpec {
    display: &'static str,
    rc_label: &'static str,
    integration_file: &'static str,
    template: &'static str,
    restart: &'static str,
    uses_zdotdir: bool,
}

const ZSH: ShellSpec = ShellSpec {
    display: "Zsh",
    rc_label: "zshrc",
    integration_file: "shmart.zsh",
    template: ZSH_PLUGIN_TEMPLATE,
    restart: "exec zsh",
    uses_zdotdir: true,
};

const BASH: ShellSpec = ShellSpec {
    display: "Bash",
    rc_label: "bashrc",
    integration_file: "shmart.bash",
    template: BASH_PLUGIN_TEMPLATE,
    restart: "exec /bin/bash",
    uses_zdotdir: false,
};

pub fn print_zsh_init() -> Result<()> {
    print_init(ZSH)
}

pub fn print_bash_init() -> Result<()> {
    print_init(BASH)
}

pub fn install_zsh(zshrc_override: Option<&Path>, dry_run: bool) -> Result<()> {
    install(ZSH, zshrc_override, dry_run)
}

pub fn install_bash(bashrc_override: Option<&Path>, dry_run: bool) -> Result<()> {
    install(BASH, bashrc_override, dry_run)
}

pub fn status_zsh(zshrc_override: Option<&Path>) -> Result<()> {
    status(ZSH, zshrc_override)
}

pub fn status_bash(bashrc_override: Option<&Path>) -> Result<()> {
    status(BASH, bashrc_override)
}

pub fn uninstall_zsh(zshrc_override: Option<&Path>) -> Result<()> {
    uninstall(ZSH, zshrc_override)
}

pub fn uninstall_bash(bashrc_override: Option<&Path>) -> Result<()> {
    uninstall(BASH, bashrc_override)
}

fn print_init(spec: ShellSpec) -> Result<()> {
    print!("{}", rendered_plugin(spec.template)?);
    Ok(())
}

fn install(spec: ShellSpec, rc_override: Option<&Path>, dry_run: bool) -> Result<()> {
    let rc_file = rc_path(spec, rc_override)?;
    let integration = integration_path(spec.integration_file)?;
    let old = read_optional(&rc_file)?;
    let source_line = format!("source {}", shell_quote(&integration.display().to_string()));
    let block = format!("{BLOCK_BEGIN}\n{source_line}\n{BLOCK_END}");
    let new = replace_managed_block(&old, Some(&block))?;

    if dry_run {
        println!(
            "Would write {} integration: {}",
            spec.display,
            integration.display()
        );
        println!("Would update: {}", rc_file.display());
        println!("\n{block}");
        return Ok(());
    }

    atomic_write(&integration, rendered_plugin(spec.template)?.as_bytes())?;
    if new != old {
        backup_if_present(&rc_file)?;
        atomic_write(&rc_file, new.as_bytes())?;
    }

    println!(
        "Installed {} integration at {}",
        spec.display,
        integration.display()
    );
    println!("Updated {}", rc_file.display());
    println!(
        "Start a new {} session or run: {}",
        spec.display, spec.restart
    );
    Ok(())
}

fn status(spec: ShellSpec, rc_override: Option<&Path>) -> Result<()> {
    let rc_file = rc_path(spec, rc_override)?;
    let integration = integration_path(spec.integration_file)?;
    let contents = read_optional(&rc_file)?;
    let configured = contents.contains(BLOCK_BEGIN) && contents.contains(BLOCK_END);

    println!("{}: {}", spec.rc_label, rc_file.display());
    println!(
        "managed source block: {}",
        if configured {
            "installed"
        } else {
            "not installed"
        }
    );
    println!("integration: {}", integration.display());
    println!(
        "integration file: {}",
        if integration.is_file() {
            "present"
        } else {
            "missing"
        }
    );
    Ok(())
}

fn uninstall(spec: ShellSpec, rc_override: Option<&Path>) -> Result<()> {
    let rc_file = rc_path(spec, rc_override)?;
    let old = read_optional(&rc_file)?;
    let new = replace_managed_block(&old, None)?;
    if new == old {
        println!("No managed shmart block found in {}", rc_file.display());
        return Ok(());
    }

    backup_if_present(&rc_file)?;
    atomic_write(&rc_file, new.as_bytes())?;
    println!(
        "Removed the managed shmart block from {}",
        rc_file.display()
    );
    println!("The integration file was retained so uninstall is recoverable.");
    Ok(())
}

fn integration_path(file_name: &str) -> Result<PathBuf> {
    let dirs = BaseDirs::new().context("could not determine the user configuration directory")?;
    Ok(dirs
        .config_dir()
        .join("shmart")
        .join("shell")
        .join(file_name))
}

fn rc_path(spec: ShellSpec, override_path: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = override_path {
        return Ok(path.to_owned());
    }
    let zdotdir = spec.uses_zdotdir.then(|| env::var_os("ZDOTDIR")).flatten();
    if let Some(zdotdir) = zdotdir {
        return Ok(PathBuf::from(zdotdir).join(".zshrc"));
    }
    let dirs = BaseDirs::new().context("could not determine the home directory")?;
    Ok(dirs.home_dir().join(if spec.uses_zdotdir {
        ".zshrc"
    } else {
        ".bashrc"
    }))
}

fn rendered_plugin(template: &str) -> Result<String> {
    let executable =
        env::current_exe().context("could not determine the shmart executable path")?;
    Ok(template.replace(
        "@SHMART_BIN@",
        &shell_quote(&executable.display().to_string()),
    ))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn read_optional(path: &Path) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(value) => Ok(value),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
    }
}

fn replace_managed_block(contents: &str, replacement: Option<&str>) -> Result<String> {
    let start = contents.find(BLOCK_BEGIN);
    let end = contents.find(BLOCK_END);
    let without = match (start, end) {
        (None, None) => contents.trim_end_matches('\n').to_owned(),
        (Some(_), None) | (None, Some(_)) => {
            bail!("found an incomplete shmart integration block; repair it manually")
        }
        (Some(start), Some(end)) if end >= start => {
            let suffix_start = end + BLOCK_END.len();
            let before = contents[..start].trim_end_matches('\n');
            let after = contents[suffix_start..].trim_matches('\n');
            match (before.is_empty(), after.is_empty()) {
                (true, true) => String::new(),
                (false, true) => before.to_owned(),
                (true, false) => after.to_owned(),
                (false, false) => format!("{before}\n\n{after}"),
            }
        }
        _ => bail!("found an invalid shmart integration block; repair it manually"),
    };

    let mut result = without;
    if let Some(block) = replacement {
        if !result.is_empty() {
            result.push_str("\n\n");
        }
        result.push_str(block);
    }
    if !result.is_empty() {
        result.push('\n');
    }
    Ok(result)
}

fn backup_if_present(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before Unix epoch")?
        .as_secs();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("shell startup path has no valid file name")?;
    let backup = path.with_file_name(format!("{file_name}.shmart-backup-{timestamp}"));
    fs::copy(path, &backup).with_context(|| {
        format!(
            "failed to back up {} to {}",
            path.display(),
            backup.display()
        )
    })?;
    println!("Backup: {}", backup.display());
    Ok(())
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .context("cannot write a path without a parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("destination path has no valid file name")?;
    let temporary = parent.join(format!(".{file_name}.shmart-tmp-{}", std::process::id()));
    fs::write(&temporary, contents)
        .with_context(|| format!("failed to write {}", temporary.display()))?;
    if let Ok(metadata) = fs::metadata(path) {
        fs::set_permissions(&temporary, metadata.permissions())
            .with_context(|| format!("failed to preserve permissions for {}", path.display()))?;
    }
    fs::rename(&temporary, path)
        .with_context(|| format!("failed to replace {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_block_is_idempotent_and_removable() {
        let original = "export EDITOR=vim\n";
        let block = format!("{BLOCK_BEGIN}\nsource '/tmp/shmart.zsh'\n{BLOCK_END}");
        let installed = replace_managed_block(original, Some(&block)).unwrap();
        assert_eq!(
            replace_managed_block(&installed, Some(&block)).unwrap(),
            installed
        );
        assert_eq!(replace_managed_block(&installed, None).unwrap(), original);
    }

    #[test]
    fn refuses_incomplete_managed_blocks() {
        assert!(replace_managed_block(BLOCK_BEGIN, None).is_err());
    }

    #[test]
    fn escapes_single_quotes_for_zsh() {
        assert_eq!(shell_quote("one'two"), "'one'\\''two'");
    }

    #[test]
    fn plugin_wraps_accept_line_and_has_no_child_shell() {
        assert!(ZSH_PLUGIN_TEMPLATE.contains("zle -N accept-line _shmart_accept_line"));
        assert!(ZSH_PLUGIN_TEMPLATE.contains("add-zsh-hook preexec"));
        assert!(ZSH_PLUGIN_TEMPLATE.contains("--argv"));
        assert!(!ZSH_PLUGIN_TEMPLATE.contains("exec zsh -c"));
    }

    #[test]
    fn bash_plugin_uses_prompt_hook_and_same_shell_eval() {
        assert!(BASH_PLUGIN_TEMPLATE.contains("PROMPT_COMMAND"));
        assert!(BASH_PLUGIN_TEMPLATE.contains("builtin eval --"));
        assert!(BASH_PLUGIN_TEMPLATE.contains("--parse-command"));
        assert!(!BASH_PLUGIN_TEMPLATE.contains("bash -c"));
    }
}
