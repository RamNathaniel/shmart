# shmart

`shmart` adds command suggestions and command-repair menus to an existing Zsh
or Bash session. It is a native shell plugin with a small Rust companion
process—not a shell, terminal emulator, command executor, or autonomous agent.

## How it works

- Ordinary input is accepted and executed by the current shell without invoking a
  model.
- `shmart <intent>` opens a suggestion menu before anything is executed.
- A likely command-usage failure opens the same menu after the command returns.
- The configured model receives plain text and returns validated JSON. It gets
  no tools and cannot execute commands.
- Rust displays the menu and returns the selected text to the integration. The
  existing Zsh or Bash process executes it, preserving `cd`, exports, aliases,
  functions, jobs, and options.

The native plugins do not capture or redirect command output. They use exit
status plus targeted shell/Git checks to avoid prompting after ordinary
failures. That classifier is part of each integration; Rust contains no second
error-detection or command-execution path.

## Build

Shmart requires Rust 1.85 or newer.

```bash
cargo build --release
```

The executable is `target/release/shmart`. The helper script builds it and
forwards its arguments:

```bash
./build-and-run.sh --help
```

## Configure the suggestion model

Interactive setup defaults to DeepSeek:

```bash
./target/release/shmart setup
```

Non-interactive examples:

```bash
shmart setup --provider deepseek
shmart setup --provider openai --model MODEL_NAME
shmart setup --provider openrouter --model provider/model-name
shmart setup \
  --provider custom \
  --endpoint https://example.test/v1/chat/completions \
  --model MODEL_NAME \
  --api-key-env EXAMPLE_API_KEY
```

Supported endpoints use the OpenAI-compatible chat-completions response shape.
DeepSeek defaults to `deepseek-v4-flash` and `DEEPSEEK_API_KEY`. API keys are
read from the named environment variable and are never stored in the config.

```bash
shmart config-path
shmart doctor
```

The current configuration format contains only the model used for menus:

```toml
[model]
endpoint = "https://api.deepseek.com/chat/completions"
model = "deepseek-v4-flash"
api_key_env = "DEEPSEEK_API_KEY"
timeout_seconds = 45
```

## Install a shell integration

With `shmart` on `PATH`:

```bash
shmart shell install zsh
exec zsh
```

From this source checkout:

```bash
./target/release/shmart shell install zsh
exec zsh
```

Installation writes the generated integration to Shmart's user configuration
directory and adds one managed source block to `${ZDOTDIR:-$HOME}/.zshrc`. An
existing `.zshrc` is backed up before modification.

```bash
shmart shell install zsh --dry-run
shmart shell status zsh
shmart shell uninstall zsh
shmart init zsh
```

`shmart init zsh` prints the integration for use with a dotfile manager.
Uninstall removes only the managed `.zshrc` block and retains the generated
integration file for recovery.

For Bash, including the `/bin/bash` 3.2 shipped with macOS:

```bash
shmart shell install bash
exec /bin/bash
```

The Bash installer writes `shmart.bash` to Shmart's integration directory and
adds the managed source block to `~/.bashrc`. If a login-only Bash setup does
not load `.bashrc`, source it from `.bash_profile` or run `source ~/.bashrc`.
Bash uses a `shmart` function for explicit requests and a `PROMPT_COMMAND` hook
for failed commands. Selected commands are evaluated by the current Bash
process, so shell state remains persistent.

```bash
shmart shell install bash --dry-run
shmart shell status bash
shmart shell uninstall bash
shmart init bash
```

Use `--rc-file FILE` to operate on a different startup file. `--zshrc` and
`--bashrc` remain readable aliases for that option.

Running `shmart` without arguments prints CLI help. It never starts another
shell or prompt.

## Use

Continue using the normal shell prompt:

```console
% cd ~/git/project
% export MODE=development
% git status
```

Ask explicitly:

```console
% shmart find the five largest files here
```

Use `shmart -- <intent>` when an intent begins with a management-command word
such as `setup` or `doctor`.

Generated commands are bold cyan, explanations are dim, menu keys are yellow,
and the FYI section is blue. Set `NO_COLOR=1` for plain output. Model-provided
terminal control characters are escaped before display.

Choose a number to execute that command in the current Zsh, enter a replacement
command, press `o` to describe another option, or press `q`/Ctrl-C to dismiss.
Ctrl-D closes menu input. Ctrl-Z retains normal job-control semantics and is not
a cancellation key.

## Study an unfamiliar Python CLI

When a failed or explicit command directly invokes `python` or `python3` with a
script or `-m module`, Shmart can learn the CLI's `argparse` structure before it
asks the model for suggestions. Detection is passive. Shmart does not import,
run, or call `--help` on the target before asking permission.

For an uncached candidate, Shmart shows the resolved target, interpreter,
working directory, and this warning:

> Shmart will run the program locally in a separate probe until it calls
> `argparse`. Code that runs before argument parsing may have side effects.

The choices are **Study this CLI once**, **Not now**, and **Don't ask again for
this CLI fingerprint**. Studying and accepting a generated command are separate
decisions; a study never executes the original or suggested command.

After approval, a disposable Python child process installs an in-process
`argparse` interception, starts the target, captures the first parser call over
a dedicated protocol pipe, and exits immediately. This is process isolation,
not a sandbox: startup code before `parse_args()` can have side effects. The
runner closes stdin, bounds output and schema sizes, removes Shmart credentials,
uses a five-second timeout, and terminates the probe process group on timeout or
cancellation.

Validated schemas are fingerprinted by the target, interpreter, environment,
working directory, and adapter version. Script content changes invalidate the
entry immediately; otherwise entries expire after seven days. Cache files use
private permissions in the platform cache directory. Sensitive defaults and
choices are redacted before caching or model use.

Manual lifecycle commands use the same consent boundary:

```bash
shmart cli study python3 tools/deploy.py --profile production
shmart cli status python3 tools/deploy.py
shmart cli schema python3 tools/deploy.py
shmart cli restudy python3 tools/deploy.py
shmart cli forget python3 tools/deploy.py
shmart cli cache list
shmart cli cache clear
```

The first version intentionally excludes `python -c`, stdin programs, wrappers
such as `sudo` and `ssh`, aliases that conceal Python, and frameworks other than
standard-library `argparse`.

## Security boundary

Shmart is not a sandbox. A selected command executes with the current user's
permissions and shell state. Review generated commands before selecting them.
