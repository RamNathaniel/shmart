use std::path::Path;

use crate::decision::Decision;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    ReadOnly,
    NeedsApproval(String),
    Blocked(String),
}

pub fn assess(decision: &Decision) -> Verdict {
    match decision {
        Decision::Shell { .. } => Verdict::NeedsApproval(
            "shell syntax can contain pipelines, expansion, redirection, or multiple commands"
                .into(),
        ),
        Decision::Run { program, args, .. } => assess_program(program, args),
        _ => Verdict::ReadOnly,
    }
}

fn assess_program(program: &str, args: &[String]) -> Verdict {
    let name = Path::new(program)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(program);

    if [
        "sudo", "doas", "su", "shutdown", "reboot", "halt", "poweroff", "fdisk", "diskutil",
    ]
    .contains(&name)
        || name.starts_with("mkfs")
    {
        return Verdict::Blocked(format!("{name} is not permitted through shmart"));
    }

    if ["sh", "bash", "zsh", "fish", "dash"].contains(&name) {
        return Verdict::Blocked("shell wrappers must use the explicit shell action".into());
    }

    if name == "find" {
        let mutating = [
            "-delete", "-exec", "-execdir", "-ok", "-okdir", "-fprint", "-fls",
        ];
        if args.iter().any(|arg| mutating.contains(&arg.as_str())) {
            return Verdict::NeedsApproval(
                "find arguments can execute commands or write files".into(),
            );
        }
        return Verdict::ReadOnly;
    }

    if name == "git" {
        let subcommand = args
            .iter()
            .find(|arg| !arg.starts_with('-'))
            .map(String::as_str);
        return match subcommand {
            Some(
                "status" | "diff" | "log" | "show" | "rev-parse" | "ls-files" | "grep" | "blame",
            ) => Verdict::ReadOnly,
            _ => Verdict::NeedsApproval(
                "this git operation may change the repository or contact a remote".into(),
            ),
        };
    }

    if name == "sort"
        && args
            .iter()
            .any(|arg| arg == "-o" || arg.starts_with("--output"))
    {
        return Verdict::NeedsApproval("sort was asked to write an output file".into());
    }

    let read_only = [
        "pwd", "ls", "rg", "grep", "cat", "head", "tail", "wc", "sort", "uniq", "cut", "tr", "du",
        "df", "ps", "pgrep", "uname", "whoami", "date", "which", "whereis", "file", "stat",
        "realpath", "readlink", "jq", "tree",
    ];
    if read_only.contains(&name) {
        Verdict::ReadOnly
    } else {
        Verdict::NeedsApproval(format!("{name} is not on the read-only allowlist"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(program: &str, args: &[&str]) -> Decision {
        Decision::Run {
            program: program.into(),
            args: args.iter().map(|arg| (*arg).into()).collect(),
            reason: "test".into(),
        }
    }

    #[test]
    fn allows_basic_inspection() {
        assert_eq!(assess(&run("/usr/bin/ls", &["-la"])), Verdict::ReadOnly);
        assert_eq!(
            assess(&run("git", &["status", "--short"])),
            Verdict::ReadOnly
        );
    }

    #[test]
    fn catches_find_delete_and_shell_wrappers() {
        assert!(matches!(
            assess(&run("find", &[".", "-delete"])),
            Verdict::NeedsApproval(_)
        ));
        assert!(matches!(
            assess(&run("bash", &["-c", "ls"])),
            Verdict::Blocked(_)
        ));
    }

    #[test]
    fn blocks_privilege_escalation() {
        assert!(matches!(
            assess(&run("sudo", &["rm", "x"])),
            Verdict::Blocked(_)
        ));
    }
}
