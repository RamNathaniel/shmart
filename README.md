# shmart

`shmart` adds command suggestions and command-repair menus to an existing Zsh
session. It is a Zsh plugin with a small Rust companion process—not a shell,
terminal emulator, command executor, or autonomous agent.

## How it works

- Ordinary input is accepted and executed by the current Zsh without invoking a
  model.
- `shmart <intent>` opens a suggestion menu before Zsh executes anything.
- A likely command-usage failure opens the same menu after the command returns.
- The configured model receives plain text and returns validated JSON. It gets
  no tools and cannot execute commands.
- Rust displays the menu and returns the selected text to ZLE. The existing Zsh
  executes it, preserving `cd`, exports, aliases, functions, jobs, and options.

The native plugin does not capture or redirect command output. It uses exit
status plus targeted Zsh/Git checks to avoid prompting after ordinary failures.
That classifier is part of the active integration; Rust contains no second
error-detection or execution path.

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

## Install the Zsh integration

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

Running `shmart` without arguments prints CLI help. It never starts another
shell or prompt.

## Use

Continue using the normal Zsh prompt:

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

## Security boundary

Shmart is not a sandbox. A selected command executes with the current user's
permissions and shell state. Review generated commands before selecting them.
