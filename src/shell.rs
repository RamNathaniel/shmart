use std::{
    env, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use directories::BaseDirs;

const PLUGIN_TEMPLATE: &str = include_str!("../shell/shmart.zsh");
const BLOCK_BEGIN: &str = "# >>> shmart shell integration >>>";
const BLOCK_END: &str = "# <<< shmart shell integration <<<";

pub fn print_zsh_init() -> Result<()> {
    print!("{}", rendered_plugin()?);
    Ok(())
}

pub fn install_zsh(zshrc_override: Option<&Path>, dry_run: bool) -> Result<()> {
    let zshrc = zshrc_path(zshrc_override)?;
    let integration = integration_path()?;
    let old = read_optional(&zshrc)?;
    let source_line = format!("source {}", zsh_quote(&integration.display().to_string()));
    let block = format!("{BLOCK_BEGIN}\n{source_line}\n{BLOCK_END}");
    let new = replace_managed_block(&old, Some(&block))?;

    if dry_run {
        println!("Would write Zsh integration: {}", integration.display());
        println!("Would update: {}", zshrc.display());
        println!("\n{block}");
        return Ok(());
    }

    atomic_write(&integration, rendered_plugin()?.as_bytes())?;
    if new != old {
        backup_if_present(&zshrc)?;
        atomic_write(&zshrc, new.as_bytes())?;
    }

    println!("Installed Zsh integration at {}", integration.display());
    println!("Updated {}", zshrc.display());
    println!("Start a new Zsh session or run: exec zsh");
    Ok(())
}

pub fn status_zsh(zshrc_override: Option<&Path>) -> Result<()> {
    let zshrc = zshrc_path(zshrc_override)?;
    let integration = integration_path()?;
    let contents = read_optional(&zshrc)?;
    let configured = contents.contains(BLOCK_BEGIN) && contents.contains(BLOCK_END);

    println!("zshrc: {}", zshrc.display());
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

pub fn uninstall_zsh(zshrc_override: Option<&Path>) -> Result<()> {
    let zshrc = zshrc_path(zshrc_override)?;
    let old = read_optional(&zshrc)?;
    let new = replace_managed_block(&old, None)?;
    if new == old {
        println!("No managed shmart block found in {}", zshrc.display());
        return Ok(());
    }

    backup_if_present(&zshrc)?;
    atomic_write(&zshrc, new.as_bytes())?;
    println!("Removed the managed shmart block from {}", zshrc.display());
    println!("The integration file was retained so uninstall is recoverable.");
    Ok(())
}

fn integration_path() -> Result<PathBuf> {
    let dirs = BaseDirs::new().context("could not determine the user configuration directory")?;
    Ok(dirs
        .config_dir()
        .join("shmart")
        .join("shell")
        .join("shmart.zsh"))
}

fn zshrc_path(override_path: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = override_path {
        return Ok(path.to_owned());
    }
    if let Some(zdotdir) = env::var_os("ZDOTDIR") {
        return Ok(PathBuf::from(zdotdir).join(".zshrc"));
    }
    let dirs = BaseDirs::new().context("could not determine the home directory")?;
    Ok(dirs.home_dir().join(".zshrc"))
}

fn rendered_plugin() -> Result<String> {
    let executable =
        env::current_exe().context("could not determine the shmart executable path")?;
    Ok(PLUGIN_TEMPLATE.replace(
        "@SHMART_BIN@",
        &zsh_quote(&executable.display().to_string()),
    ))
}

fn zsh_quote(value: &str) -> String {
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
        .context("zshrc path has no valid file name")?;
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
        assert_eq!(zsh_quote("one'two"), "'one'\\''two'");
    }

    #[test]
    fn plugin_wraps_accept_line_and_has_no_child_shell() {
        assert!(PLUGIN_TEMPLATE.contains("zle -N accept-line _shmart_accept_line"));
        assert!(PLUGIN_TEMPLATE.contains("add-zsh-hook preexec"));
        assert!(PLUGIN_TEMPLATE.contains("--argv"));
        assert!(!PLUGIN_TEMPLATE.contains("exec zsh -c"));
    }
}
