# smartsh

`smartsh` is a terminal assistant with a two-tier inference design:

- By default, IBM Granite 4.2 3B runs locally and makes low-latency routing decisions; the immediate tier can also use an authenticated OpenAI-compatible endpoint.
- An optional cloud model handles requests that need heavier reasoning.
- Rust owns command execution, approval, timeouts, and policy. The model never bypasses that layer.

The current release includes an initial interactive command loop. It deliberately executes ordinary commands as an executable plus an argument array. Pipelines and redirection use a separate `shell` action and always require confirmation.

## Requirements

- Rust 1.85 or newer (the crate uses Rust 2024 edition).
- A current `llama.cpp` installation with Metal support on Apple Silicon.
- Approximately 3–4.5 GB of free unified memory for Granite at an 8K context.

## Build

```bash
cargo build --release
```

The resulting binary is `target/release/smartsh`.

Build and immediately run Smartsh with the included helper:

```bash
./build-and-run.sh
```

Arguments are forwarded, for example `./build-and-run.sh --verbose` or `./build-and-run.sh doctor`.

## Configure

Interactive setup:

```bash
smartsh setup
```

The setup supports OpenAI, Anthropic, OpenRouter, DeepSeek, a custom OpenAI-compatible endpoint, or no cloud model. API keys are referenced by environment-variable name and are never written to the configuration file.

Non-interactive examples:

```bash
smartsh setup --provider openai --model YOUR_MODEL_NAME
smartsh setup --provider anthropic --model YOUR_MODEL_NAME
smartsh setup --provider openrouter --model provider/model-name
smartsh setup --provider deepseek
smartsh setup --provider none
```

The DeepSeek preset defaults to the rolling `deepseek-v4-flash` API model.

Smartsh does not provide tools or terminal access to any model. For DeepSeek and
other OpenAI-compatible endpoints, it sends ordinary system/user messages,
requests JSON mode, and includes the expected schema as prompt text. If an
endpoint does not implement JSON mode, Smartsh retries with prompt-only JSON
instructions. Rust validates every response before rendering a menu or acting
on a decision.

For a custom endpoint:

```bash
smartsh setup \
  --provider custom \
  --model my-model \
  --endpoint https://example.test/v1/chat/completions \
  --api-key-env EXAMPLE_API_KEY
```

To temporarily use DeepSeek Flash for both immediate routing and heavy reasoning:

```bash
smartsh setup \
  --provider deepseek \
  --model deepseek-v4-flash \
  --local-endpoint https://api.deepseek.com/chat/completions \
  --local-model deepseek-v4-flash \
  --local-api-key-env DEEPSEEK_API_KEY
```

Print the active configuration location with:

```bash
smartsh config-path
```

## Start Granite

The default local model is the official Q4_K_M GGUF:

```bash
llama serve \
  -hf ibm-granite/granite-4.2-3b-GGUF:Q4_K_M \
  -c 8192 \
  -ngl 99 \
  --jinja \
  --port 8080
```

The first launch downloads the model from Hugging Face. Keep the local server running in a separate terminal.

Then check the installation:

```bash
smartsh doctor
```

## Use

Start an interactive session by running Smartsh without a subcommand:

```bash
smartsh
```

Enter shell commands at the `smartsh>` prompt. Normal lines execute literally without contacting an LLM. Smartsh invokes the configured immediate model only after output indicates a likely command-usage error, such as an unknown command or invalid option.

Prefix an intent with the reserved `shmart` keyword to request assistance immediately:

```text
smartsh> shmart find the five largest files here
```

Smartsh presents a numbered menu of commands with short explanations. Choose an option, type a replacement command, press `o` to describe the task another way and get a fresh menu, or press `q` to dismiss it without running anything. The menu ends with an FYI section for relevant optional tools and their install commands; these are informational and are never run automatically. Normal shell invocations such as `sh script.sh` and `/bin/sh script.sh` execute normally.

Use `help` for session commands, `exit` or Ctrl-D to leave, and Ctrl-C to cancel the current input. The editor supports multiline input, history navigation, and normal line editing through Reedline.

Normal mode keeps internal routing details quiet. Start with `smartsh --verbose` to show routing choices, command explanations, and cloud/local fallback diagnostics.

Commands such as `ls`, `pwd`, pipelines, redirection, and `git status` bypass the LLM and run literally, as required by the workplan's Enter-key semantics. `cd` changes the interactive session's working directory.

The one-shot form remains available:

```bash
smartsh ask "show the five largest files in this directory"
smartsh ask "explain why the last build failed"
smartsh ask --dry-run "remove generated log files older than seven days"
smartsh ask --no-cloud "show the current git branch"
```

Read-only commands from a conservative allowlist run immediately. Unknown or mutating programs require confirmation. Privilege escalation, shell-wrapper bypasses, and disk-management commands are blocked.

When Granite requests heavier reasoning, Smartsh prefers the configured cloud model. If cloud delegation is disabled, lacks credentials, returns an invalid delegation, or cannot be reached, Granite automatically continues the task locally.

## Configuration

The generated TOML contains three sections:

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

Keep `auto_execute_read_only = false` if every command should require confirmation.

## Security boundary

`smartsh` is not a sandbox. The policy layer reduces accidental execution but cannot make arbitrary commands harmless. Run it inside a restricted working directory or OS sandbox when processing untrusted requests or terminal output.
