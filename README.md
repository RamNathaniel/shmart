# shmart

`shmart` is a low-latency terminal assistant that integrates with your existing
Zsh session. It is not a shell and does not open a nested shell.

- Ordinary commands run normally in Zsh without contacting an LLM.
- `shmart <intent>` asks for help before anything runs.
- After a likely command-usage failure, Shmart offers corrected commands.
- A selected command is returned to Zsh's editable buffer and executed by the
  same persistent shell.
- The model receives no tools or terminal access. It returns validated JSON;
  Rust renders the menu, and the Zsh integration performs the selected action.

This means shell state persists across every command, including `cd`, exported
variables, aliases, functions, jobs, and shell options.

## Requirements

- macOS with Zsh for the initial native integration.
- Rust 1.85 or newer to build from source.
- Either a local OpenAI-compatible model server or an authenticated compatible
  endpoint.
- For the default local setup, a current `llama.cpp` build with Metal support
  and roughly 3–4.5 GB of free unified memory.

## Build

```bash
cargo build --release
```

The resulting binary is `target/release/shmart`. The helper builds and forwards
any arguments:

```bash
./build-and-run.sh --help
```

For local development, either copy the binary onto `PATH` or invoke its absolute
path when installing the integration. The generated plugin remembers that path
and exposes the `shmart` management command in the shell when no existing
command or function has that name.

## Configure models

Run interactive setup:

```bash
shmart setup
```

Cloud choices include OpenAI, Anthropic, OpenRouter, DeepSeek, a custom
OpenAI-compatible endpoint, or no cloud model. If cloud delegation is disabled
or unavailable, the immediate model also handles heavier reasoning.

Examples:

```bash
shmart setup --provider deepseek
shmart setup --provider none
shmart setup --provider openai --model YOUR_MODEL_NAME
```

To use DeepSeek Flash for both tiers:

```bash
shmart setup \
  --provider deepseek \
  --model deepseek-v4-flash \
  --local-endpoint https://api.deepseek.com/chat/completions \
  --local-model deepseek-v4-flash \
  --local-api-key-env DEEPSEEK_API_KEY
```

API keys are referenced by environment-variable name and are never stored in
the configuration file. Print its location or check connectivity with:

```bash
shmart config-path
shmart doctor
```

Existing configuration under the old `smartsh` application directory is copied
to the new `shmart` directory on first use. The legacy file is retained.

## Start the default local model

```bash
llama serve \
  -hf ibm-granite/granite-4.2-3b-GGUF:Q4_K_M \
  -c 8192 \
  -ngl 99 \
  --jinja \
  --port 8080
```

The first launch downloads the model from Hugging Face. Keep the server running
in a separate terminal.

## Install the Zsh integration

With the binary on `PATH`, run:

```bash
shmart shell install zsh
exec zsh
```

From this source checkout, the equivalent command is:

```bash
./target/release/shmart shell install zsh
```

Installation writes the versioned integration to Shmart's user configuration
directory and appends one idempotent managed block to `${ZDOTDIR:-$HOME}/.zshrc`.
If `.zshrc` already exists, it is backed up before modification.

Useful management commands:

```bash
shmart shell install zsh --dry-run
shmart shell status zsh
shmart shell uninstall zsh
shmart init zsh                    # print integration for a dotfile manager
```

Pass `--zshrc FILE` to the install, status, or uninstall command to operate on a
different file. Uninstall removes only Shmart's managed source block and leaves
the generated integration file in place for recovery.

Running `shmart` without arguments prints CLI help; it does not create another
shell or another prompt.

## Use

Continue using the normal Zsh prompt. These lines bypass the LLM and execute
normally:

```console
% cd ~/git/project
% export MODE=development
% git status
```

Ask for command suggestions explicitly:

```console
% shmart find the five largest files here
```

Shmart intercepts that line before Zsh parses it and displays two to four
options. Commands are bold cyan, explanations are dim, menu control keys are
yellow, and the FYI section is blue. Selecting a command returns it to the
current Zsh line editor and accepts it, so the same Zsh process executes it.
Set `NO_COLOR=1` to disable terminal styling. Use `shmart -- <intent>` when the
intent begins with a Shmart management-command word such as `setup` or
`doctor`.

The menu temporarily switches the terminal from ZLE's raw input mode to normal
canonical input. Enter and editing behave normally, Ctrl-C cancels, and Ctrl-D
closes the menu input. ZLE's prior terminal mode is restored when the menu
exits. Ctrl-Z retains its ordinary job-control meaning and is not a cancellation
key; use Ctrl-C or `q` to dismiss the menu.

After likely usage errors, Shmart opens the same menu automatically. The first
release detects standard command-not-found and usage exit statuses, Zsh glob
parse failures, and unknown Git subcommands. Native integration intentionally
does not capture or redirect stderr because doing so would break normal
interactive terminal behavior; consequently, no exit-status-only heuristic can
identify every usage error.

The menu's FYI section may suggest optional tools and installation commands.
Those entries are informational and cannot be selected or executed by Shmart.

The one-shot agent remains available for scripted or legacy use:

```bash
shmart ask "show the five largest files in this directory"
shmart ask --dry-run "remove generated log files older than seven days"
shmart ask --no-cloud "show the current git branch"
```

## Model boundary

Shmart sends plain system and user messages and asks for a JSON object matching
an explicit schema. It does not provide function tools, shell tools, filesystem
tools, or network tools to either model. Rust strictly validates every response
before displaying it. In native Zsh mode, Rust also never executes a selected
suggestion; it writes that selection to a private temporary handoff file, and
the Zsh line editor executes it in the current shell.

## Configuration

The generated TOML contains:

```toml
[local]
endpoint = "http://127.0.0.1:8080/v1/chat/completions"
model = "ibm-granite/granite-4.2-3b-GGUF:Q4_K_M"
api_key_env = ""
timeout_seconds = 45

[cloud]
enabled = false
provider = "open_ai_compatible"
endpoint = ""
model = ""
api_key_env = ""
timeout_seconds = 120

[behavior]
max_steps = 6
output_limit_bytes = 16384
command_timeout_seconds = 30
auto_execute_read_only = true
```

## Security boundary

`shmart` is not a sandbox. Always review generated commands and use `q` to
dismiss a menu. The native integration executes selected suggestions with your
current user's permissions and current shell state.
