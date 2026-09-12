# Smartsh — Product Plan and Technical Specification

**Status:** Draft v0.1  
**Date:** 2026-09-07  
**Target platforms:** Linux and macOS  
**Primary implementation language:** Rust  
**Working product name:** Smartsh

---

## 1. Executive Summary

Smartsh is an AI-native interactive shell frontend for Linux and macOS.

It is **not** a new terminal emulator and it is **not** a new shell language. Instead, Smartsh sits between an existing terminal emulator and an existing shell such as Zsh or Bash. It owns the interactive command-editing experience, contextual history, AI-assisted completion and rewriting, command explanation, and—eventually—persistent PTY sessions that survive SSH disconnects.

The core product idea is that AI assistance should be **ambient and proactive**, rather than requiring the user to stop what they are doing and explicitly invoke a chatbot.

A user can type any mixture of:

- exact shell syntax,
- partial commands,
- half-remembered syntax,
- natural-language intent,
- previously used command fragments,
- commands copied from somewhere else.

Smartsh continuously decides whether the best action is to:

1. do nothing,
2. provide a conventional completion,
3. offer an inline suggestion,
4. rewrite part of the command,
5. rewrite the entire command,
6. show a ranked menu of relevant alternatives,
7. explain what the current or proposed command will do,
8. surface a warning about dangerous or surprising behavior.

The key product distinction is:

> **The LLM is not merely a command generator. It is a UI-policy and intent-understanding engine that decides when and how to assist.**

Every LLM-generated executable proposal must include a human-readable explanation of what it will do. Potentially dangerous proposals must also include explicit warnings.

Smartsh should preserve the strengths of the normal terminal environment:

- native terminal scrollback,
- native mouse selection,
- normal copy/paste,
- fast keyboard interaction,
- full compatibility with interactive programs such as Vim, SSH, `top`, Python, `less`, etc.,
- compatibility with Bash and Zsh,
- resilience to SSH disconnects,
- no requirement for a custom terminal application.

---

# 2. Product Thesis

Traditional shells have three major usability problems:

1. **Syntax recall**  
   Users often know what they want to do but do not remember the exact command-line syntax.

2. **Discovery**  
   CLI tools frequently expose dozens or hundreds of flags and subcommands. Users interrupt their workflow to read `man`, `--help`, documentation, search engines, or Stack Overflow.

3. **History is underused**  
   Traditional shell history is usually little more than a chronological list of command strings. It does not adequately exploit context such as project, directory, host, branch, previous command outcome, or semantic similarity.

Current AI command tools typically solve a different problem:

```text
user explicitly invokes AI
        ↓
user asks a question
        ↓
AI returns a command
        ↓
user copies/runs it
```

Smartsh aims for:

```text
user types normally
        ↓
Smartsh observes command-buffer + context
        ↓
Smartsh decides whether assistance would help
        ↓
nothing / completion / rewrite / menu / explanation
        ↓
user remains in normal shell workflow
```

The product should feel like a normal shell that has unusually good instincts.

---

# 3. Product Goals

## 3.1 Primary goals

Smartsh SHALL:

1. Work inside existing terminal emulators on Linux and macOS.
2. Preserve native terminal scrollback, selection, and mouse-based copying.
3. Use Bash and/or Zsh for actual shell semantics and execution.
4. Provide very low-latency interactive editing.
5. Use recent and contextual command history.
6. Understand natural-language intent inside the command buffer.
7. Be able to replace/rewrite the entire command buffer.
8. Provide conventional deterministic completions when possible.
9. Use an LLM to rank, interpret, rewrite, explain, and decide when to intervene.
10. Provide an explanation for every LLM-generated executable command.
11. Warn about dangerous or destructive command behavior.
12. Never execute an LLM-generated command without first placing it in visible, editable form.
13. Transparently pass control to interactive applications.
14. Eventually provide persistent PTY sessions that survive client/SSH disconnects.
15. Support both local and cloud LLM providers behind a common model interface.
16. Allow privacy-sensitive users to run fully locally.

## 3.2 Secondary goals

Smartsh SHOULD:

- learn user-specific patterns from local history,
- understand repository and machine context,
- prefer deterministic knowledge over LLM hallucination when command schemas are known,
- support multiple concurrent sessions,
- support configurable model providers,
- offer an inspectable debug mode,
- remain useful when AI is disabled or unavailable,
- operate gracefully over high-latency SSH links.

---

# 4. Non-Goals

At least for initial releases, Smartsh is NOT intended to:

1. Implement a new shell scripting language.
2. Reimplement all Bash or Zsh parsing and semantics.
3. Replace iTerm2, Terminal.app, WezTerm, Kitty, Alacritty, or other terminal emulators.
4. Provide a graphical desktop UI.
5. Automatically execute AI-generated commands.
6. Act as an autonomous software agent.
7. Modify commands after the user has pressed Enter without showing the modification.
8. Capture the mouse for normal interaction.
9. Replace tmux in the first production milestone.
10. Guarantee syntactic understanding of every possible shell extension.
11. Upload shell history or environment data to cloud providers without explicit configuration.

---

# 5. Product Invariants

The following should be treated as architectural invariants unless deliberately revised.

## 5.1 Terminal owns the mouse

Smartsh SHALL NOT enable terminal mouse-capture mode during ordinary use.

Therefore:

```text
keyboard ───► Smartsh
mouse ──────► terminal emulator
```

This preserves:

- wheel/trackpad scrolling,
- drag selection,
- native terminal copy behavior,
- terminal-specific text selection shortcuts.

Menus SHALL be fully keyboard-driven.

Recommended keys:

| Key | Default behavior |
|---|---|
| Up / Down | move through candidates |
| Tab | accept current suggestion |
| Shift-Tab | previous candidate / alternate action |
| Esc | dismiss suggestion/menu |
| Enter | execute typed command or stage an AI rewrite, depending on state |
| Ctrl-R | contextual history search |
| Ctrl-Space | explicitly request AI assistance |
| Alt-E | explain current command |
| Alt-A | show alternatives |

Exact bindings are configurable.

## 5.2 No permanent alternate-screen UI

Smartsh SHALL NOT behave like a full-screen TUI during ordinary command editing.

The normal terminal scrollback should contain command output exactly as the user expects.

Transient suggestion UI should be rendered near the current prompt and removed/repainted as needed.

## 5.3 Shell remains the authority for shell semantics

Smartsh may parse enough syntax to provide editing and assistance, but actual execution semantics belong to Bash/Zsh.

## 5.4 AI commands are staged, not silently executed

LLM output must enter an editable proposal state before execution.

## 5.5 Explanation accompanies generated execution

Any LLM response that produces an executable shell command must provide:

- `command`
- `what_it_does`

and may also provide:

- `why_suggested`
- `warnings`
- `assumptions`
- `alternatives`

## 5.6 Low latency beats cleverness

No network or model request may block keyboard echo or basic editing.

---

# 6. Core UX Model

Smartsh should consider the command buffer a mixture of **syntax and intent**, not merely a shell program.

Examples:

```text
git sta
```

may be completed conventionally:

```text
git status
```

while:

```text
git interactive rebase on master branch
```

may be interpreted as intent and rewritten to:

```bash
git rebase -i master
```

with an explanation such as:

> Interactively rebases commits on the current branch that are not already reachable from `master`. The interactive editor allows commits to be reordered, squashed, edited, or dropped. This rewrites commit history.

A mixed expression:

```text
kubectl get pods sorted by creation time
```

may become:

```bash
kubectl get pods --sort-by=.metadata.creationTimestamp
```

A half-correct command:

```text
tar extract archive.tar.gz
```

may become:

```bash
tar -xzf archive.tar.gz
```

The distinction between “natural language” and “shell command” should not require an explicit mode switch.

---

# 7. Interaction Types

The Smartsh decision engine should output one of a small number of UI actions.

```rust
enum UiAction {
    Nothing,
    InlineSuggestion,
    CompletionMenu,
    PartialRewrite,
    WholeLineRewrite,
    AlternativesMenu,
    Explanation,
    Warning,
}
```

A future version may support compound actions, but the initial interaction model should remain simple.

## 7.1 Nothing

The most important valid output.

Smartsh should avoid becoming noisy.

Example:

```bash
git status
```

If the command is clear, common, valid, and not dangerous, the correct intervention is usually no intervention.

## 7.2 Inline suggestion

Used for high-confidence, low-surprise completions.

```text
git sta|tus
```

Ghost text should be visually distinguishable and must not execute until accepted.

## 7.3 Completion menu

Used when several structurally valid candidates are relevant.

Example:

```text
git rebase --
    --interactive
    --onto
    --continue
    --abort
```

LLM ranking can order the list, but candidate validity should preferably come from deterministic sources.

## 7.4 Partial rewrite

Example:

```text
git rebase interactive master
```

could replace the relevant phrase with:

```text
git rebase -i master
```

## 7.5 Whole-line rewrite

Example:

```text
find files changed in last two days that contain foobar
```

becomes:

```bash
find . -type f -mtime -2 -exec grep -l 'foobar' {} +
```

This is a first-class feature, not a fallback.

## 7.6 Alternatives menu

Example:

```text
remove all docker containers that are stopped
```

could offer:

```text
1. docker container prune
   Removes all stopped containers after confirmation.

2. docker rm $(docker ps -aq --filter status=exited)
   Explicitly selects exited containers and removes them.
```

## 7.7 Explanation

Can explain either:

- a Smartsh proposal,
- a command typed by the user,
- a command retrieved from history.

## 7.8 Warning

Example:

```bash
git reset --hard origin/main
```

Smartsh may show:

> WARNING: resets the working tree and index to `origin/main`; uncommitted changes are discarded.

Warnings are advisory. Smartsh should not become an intrusive policy engine.

---

# 8. Enter-Key Semantics

Enter behavior is critical.

## 8.1 Clearly executable command

If the buffer appears to be an explicit shell command:

```bash
git status
```

Enter executes it immediately.

## 8.2 Ghost completion exists

If the visible buffer is:

```text
git sta█tus
```

where `tus` is ghost text, pressing Enter executes only the actual buffer unless the suggestion has already been accepted.

## 8.3 Buffer is clearly intent rather than executable syntax

Example:

```text
git interactive rebase on master branch
```

First Enter:

1. requests or accepts an AI rewrite,
2. replaces/stages the command,
3. shows explanation,
4. does NOT execute.

Result:

```text
$ git rebase -i master

  Interactively rebases commits on the current branch relative to master.
  This can rewrite commit history.

  Enter: run    Esc: edit/dismiss
```

Second Enter executes.

## 8.4 Ambiguous case

When uncertain, Smartsh should favor the user's literal command.

A valid command must never be replaced silently because Smartsh thinks a better command exists.

---

# 9. High-Level Architecture

```text
┌──────────────────────────────────────────────────────────────┐
│ Existing terminal emulator                                   │
│ Terminal.app / iTerm2 / WezTerm / Kitty / Alacritty / etc.  │
│                                                              │
│ Owns: rendering surface, scrollback, mouse selection         │
└──────────────────────────────┬───────────────────────────────┘
                               │ keyboard / terminal bytes
                               ▼
┌──────────────────────────────────────────────────────────────┐
│ Smartsh Client                                                │
│                                                              │
│  ┌───────────────┐  ┌────────────────┐  ┌────────────────┐  │
│  │ Line editor   │  │ Suggestion UI  │  │ Input routing  │  │
│  └───────┬───────┘  └────────────────┘  └───────┬────────┘  │
│          │                                        │           │
│          ▼                                        │           │
│  ┌──────────────────────────┐                     │           │
│  │ Decision / completion    │                     │           │
│  │ coordinator              │                     │           │
│  └────────────┬─────────────┘                     │           │
└───────────────┼───────────────────────────────────┼───────────┘
                │                                   │
                │ local IPC                         │ PTY I/O
                ▼                                   ▼
┌──────────────────────────────────────────────────────────────┐
│ Smartsh Service / Daemon                                      │
│                                                              │
│ Context      History      LLM providers     Session manager   │
│ providers    database     local/cloud       PTY/tmux bridge   │
└──────────────────────────────┬───────────────────────────────┘
                               │
                               ▼
                   ┌──────────────────────┐
                   │ Bash / Zsh           │
                   │ real shell semantics │
                   └──────────┬───────────┘
                              │
                              ▼
                   programs / SSH / Vim / Git / etc.
```

The daemon boundary can be introduced incrementally. Early prototypes may run more functionality in-process.

---

# 10. Recommended Implementation Strategy

## 10.1 Language

Rust is the recommended language because the project requires:

- low startup latency,
- predictable memory usage,
- PTY/process handling,
- terminal event handling,
- asynchronous tasks,
- safe concurrency,
- easy distribution as a native binary.

## 10.2 Suggested ecosystem components

These are recommendations, not hard dependencies.

### Reedline

Reedline is a Rust line editor developed primarily for Nushell. Current releases provide editing, configurable bindings, history, completion, syntax highlighting, hints, undo, clipboard integration, multiline support, and SQLite-backed richer history.

Recommended approach:

- prototype with Reedline,
- extend/fork only when Smartsh requires behavior that its abstractions cannot support cleanly.

Potential concern:

Reedline's own documentation identifies slow/concurrent completion and full-duplex concurrent output as areas where further improvement is useful. Smartsh's asynchronous model architecture may therefore require upstream work, a fork, or a thinner custom editor later.

### Crossterm

Recommended for portable terminal event/input handling.

Smartsh should use:

- keyboard events,
- resize events,
- bracketed paste,
- raw mode when editing.

Smartsh should **not enable mouse capture**.

Bracketed paste should be supported so multi-line pasted content is treated as one logical paste and can be validated appropriately.

### portable-pty

Recommended as a PTY abstraction for Linux/macOS.

### Tokio

Recommended asynchronous runtime for:

- model requests,
- cancellation,
- timers/debounce,
- IPC,
- provider queries,
- contextual subprocesses.

### SQLite

Recommended for:

- command history,
- contextual metadata,
- learned rankings,
- model interaction metadata,
- session metadata.

### tmux

Recommended only as the first resilience backend.

The final architecture should not depend on users knowing that tmux is present.

---

# 11. Line Editor Specification

The line editor is the primary interactive component.

It SHALL support:

- insertion/deletion,
- cursor movement,
- word movement,
- Home/End,
- multiline input,
- undo/redo,
- history navigation,
- reverse/contextual history search,
- configurable Emacs-like bindings,
- optional Vi mode,
- bracketed paste,
- Unicode,
- terminal resize,
- inline ghost suggestions,
- transient menu rendering,
- replacement of arbitrary buffer spans,
- whole-buffer replacement,
- explanation panel rendering.

## 11.1 Edit representation

Do not model AI completion merely as “suffix text”.

Use explicit edits:

```rust
struct TextEdit {
    start_byte: usize,
    end_byte: usize,
    replacement: String,
}
```

Examples:

### suffix completion

```text
buffer: git sta
edit:   [7..7] -> "tus"
```

### partial rewrite

```text
buffer: git rebase interactive master
edit:   [11..22] -> "-i"
```

### whole-line rewrite

```text
edit: [0..buffer.len()] -> "git rebase -i master"
```

This becomes a general mechanism usable by:

- deterministic completion,
- AI rewrite,
- history recovery,
- automatic repair.

## 11.2 Diff rendering

For whole-line transformations, Smartsh SHOULD optionally show a compact diff.

Example:

```text
git interactive rebase on master branch
───────────────────────────────────────
git rebase -i master
```

For small changes:

```text
git rebase master
          +++
→ git rebase -i master
```

Exact visual design should be user-tested.

---

# 12. Input Routing and Transparent Program Mode

Smartsh has two major states.

## 12.1 Editing state

Smartsh owns keyboard input.

```text
terminal → Smartsh editor
```

## 12.2 Passthrough state

Once a command starts a foreground program, Smartsh becomes transparent.

```text
terminal keyboard → PTY → program
program output     → PTY → terminal
```

This is required for:

- Vim/Neovim,
- Emacs,
- `less`,
- `top`/`htop`,
- Python REPL,
- SSH,
- database clients,
- TUIs,
- interactive installers.

Smartsh should not attempt to interpret keystrokes while another foreground process owns the terminal.

When the shell becomes ready for another command, Smartsh returns to editing state.

---

# 13. Shell Integration Protocol

Smartsh needs to know:

- when a prompt begins,
- when a command begins,
- when a command finishes,
- current working directory,
- exit status,
- possibly execution duration and other context.

This should be implemented through small Bash and Zsh integration scripts.

Conceptually:

```text
SMARTSH_PROMPT_START
SMARTSH_COMMAND_START
SMARTSH_COMMAND_END
```

The actual transport should use non-printing control sequences or a dedicated file descriptor / IPC mechanism.

Recommended priorities:

1. Prefer existing semantic prompt protocols where suitable.
2. Add a private Smartsh extension for data not represented by standard markers.
3. Do not print visible protocol data into scrollback.

Possible contextual payload:

```json
{
  "event": "prompt_ready",
  "cwd": "/Users/ram/src/project",
  "exit_code": 0,
  "duration_ms": 317,
  "shell": "zsh",
  "shell_pid": 1234
}
```

## 13.1 Zsh integration

Possible hooks include:

- `precmd`
- `preexec`
- directory-change hooks

## 13.2 Bash integration

Possible mechanisms include:

- `PROMPT_COMMAND`
- DEBUG traps where appropriate
- explicit PS1 semantic markers

The integration must be robust when users have complex existing prompts.

---

# 14. Completion and Knowledge Architecture

Smartsh should not ask an LLM to reinvent known CLI syntax.

Completion candidates should come from multiple providers.

```text
                  command buffer
                       │
       ┌───────────────┼─────────────────┐
       ▼               ▼                 ▼
 shell completion  structured specs   filesystem
       │               │                 │
       └───────────────┼─────────────────┘
                       ▼
                 candidate pool
                       │
                       ▼
                ranking / LLM
```

## 14.1 Provider types

Recommended providers:

1. shell-native completion,
2. structured CLI specifications,
3. filesystem paths,
4. executable discovery from `PATH`,
5. Git repository information,
6. environment-aware providers,
7. history,
8. command-specific plugins,
9. LLM synthesis.

## 14.2 Carapace

Carapace is worth evaluating as a bridge to existing shell completion systems. Its current documentation describes bridging Bash, Fish, Zsh, Inshellisense, and several other completion ecosystems.

Treat it as an optional provider, not a hard dependency.

## 14.3 Provider API

```rust
#[async_trait]
trait CompletionProvider {
    async fn complete(&self, ctx: &CompletionContext)
        -> Result<Vec<Candidate>>;
}
```

```rust
struct Candidate {
    edit: TextEdit,
    display: String,
    description: Option<String>,
    source: CandidateSource,
    confidence: f32,
    metadata: serde_json::Value,
}
```

---

# 15. History System

History is a first-class contextual database, not merely a text file.

Atuin provides a useful reference model: its current documentation describes SQLite history with context such as directory, command duration, success/failure, machine, and session.

Smartsh should store at least:

```text
id
timestamp_start
timestamp_end
command
cwd
exit_code
duration_ms
hostname_id
session_id
shell
```

Recommended optional fields:

```text
git_repository_id
git_branch
ssh_context
command_family
interactive
source
accepted_ai_proposal_id
```

## 15.1 Suggested SQLite schema

```sql
CREATE TABLE history (
    id                  INTEGER PRIMARY KEY,
    started_at          INTEGER NOT NULL,
    finished_at         INTEGER,
    command             TEXT NOT NULL,
    cwd                 TEXT NOT NULL,
    exit_code           INTEGER,
    duration_ms         INTEGER,
    host_id             TEXT,
    session_id          TEXT NOT NULL,
    shell               TEXT NOT NULL,
    git_repo            TEXT,
    git_branch          TEXT,
    source              TEXT NOT NULL DEFAULT 'user',
    proposal_id         TEXT
);

CREATE INDEX history_started_at_idx ON history(started_at DESC);
CREATE INDEX history_cwd_idx ON history(cwd);
CREATE INDEX history_repo_idx ON history(git_repo);
CREATE INDEX history_session_idx ON history(session_id);
```

## 15.2 Ranking historical commands

A first deterministic scorer can use:

```text
score =
    prefix_similarity
  + recency_weight
  + frequency_weight
  + same_cwd_weight
  + same_repo_weight
  + same_host_weight
  + same_session_weight
  + success_weight
  + semantic_similarity
```

Weights should be learned/tuned empirically.

## 15.3 Semantic embeddings

Optional, later milestone.

Commands may be embedded locally and stored in a vector index.

Avoid adding vector infrastructure until conventional contextual ranking proves insufficient.

---

# 16. Context System

The LLM should receive a carefully selected context package, not a dump of the user's machine.

## 16.1 Core context

```rust
struct Context {
    buffer: String,
    cursor: usize,
    cwd: PathBuf,
    previous_exit_code: Option<i32>,
    recent_history: Vec<HistoryItem>,
    relevant_history: Vec<HistoryItem>,
    shell: ShellInfo,
    host: HostInfo,
    repo: Option<RepoContext>,
    command_context: Option<CommandContext>,
}
```

## 16.2 Git context

Useful information:

```text
repository root
current branch
default branch
dirty/clean state
upstream branch
recent branch names
```

Do not run expensive Git commands for every keystroke.

Cache repository state and refresh on relevant events.

## 16.3 Filesystem context

Potentially useful:

- current directory file names,
- executable files,
- project markers,
- selected well-known configuration files.

Apply strict limits.

Never recursively crawl arbitrary working trees during interactive typing.

## 16.4 Environment variables

Do **not** send the full environment to an LLM.

Many environments contain:

- API keys,
- tokens,
- passwords,
- private endpoints,
- credentials.

Use an explicit allowlist such as:

```text
TERM
SHELL
LANG
EDITOR
VISUAL
```

Command-specific providers may expose carefully selected additional values.

---

# 17. LLM Architecture

The LLM subsystem consists of:

1. intervention policy,
2. intent interpretation,
3. command synthesis/rewrite,
4. explanation,
5. risk identification,
6. candidate ranking.

These functions can initially be served by one model, but the architecture should not assume they always will be.

The model boundary is deliberately capability-free:

1. Smartsh MUST NOT provide function tools, shell tools, filesystem tools, or network tools to any model.
2. The model receives ordinary prompt messages containing the command line, relevant error output and limited terminal context, plus the expected JSON shape.
3. The model returns inert JSON containing command options, explanations and optional recommendations.
4. Rust parses and validates that JSON, renders the menu, applies policy and runs only the command selected or approved by the user.

---

# 18. Model Output Contract

The model MUST return structured data.

A proposal should resemble:

```rust
struct Proposal {
    id: Uuid,
    action: ProposalAction,
    edits: Vec<TextEdit>,
    command: Option<String>,
    what_it_does: Option<String>,
    why_suggested: Option<String>,
    warnings: Vec<Warning>,
    assumptions: Vec<String>,
    alternatives: Vec<Alternative>,
    confidence: f32,
}
```

```rust
enum ProposalAction {
    Nothing,
    Inline,
    Menu,
    PartialRewrite,
    WholeRewrite,
    ExplainOnly,
    WarnOnly,
}
```

```rust
struct Warning {
    severity: WarningSeverity,
    message: String,
}
```

## 18.1 Mandatory explanation rule

If:

```text
proposal.command != None
```

and the proposal came from an LLM, then:

```text
proposal.what_it_does != None
```

is mandatory.

Malformed model responses should be rejected or repaired.

---

# 19. Explanation Specification

Explanations should describe **semantics**, not merely restate syntax.

Bad:

> Runs git rebase with interactive mode.

Good:

> Opens an interactive rebase for commits on the current branch that are not already part of `master`. You can reorder, squash, edit, or drop those commits. This rewrites commit history.

For:

```bash
rsync -azP --delete ./dist/ prod:/srv/www/
```

useful explanation:

> Recursively copies `./dist/` to `prod:/srv/www/`, preserving common metadata, using compression and showing transfer progress. `--delete` removes files at the destination that no longer exist in the source.

## 19.1 Two explanation dimensions

Store separately:

### `what_it_does`

User-facing semantics.

### `why_suggested`

Why Smartsh chose this proposal.

Example:

```text
what_it_does:
Interactively rebases the current branch relative to main.

why_suggested:
The current buffer appears to describe an intended Git operation
rather than literal Git syntax.
```

The second field may be hidden in normal use.

---

# 20. Model Invocation Strategy

No model request may block editing.

Recommended pipeline:

```text
keystroke
   │
   ├── immediately update screen
   │
   ├── deterministic completion
   │
   └── debounce
          │
          ▼
   cancel obsolete request
          │
          ▼
   gather bounded context
          │
          ▼
       model request
          │
          ▼
   ignore response if buffer changed
```

## 20.1 Request versioning

Every asynchronous request should contain:

```text
buffer_revision
session_id
request_id
```

A returned proposal is accepted only if it still corresponds to the current compatible buffer revision.

## 20.2 Cancellation

Model calls should be aggressively cancellable.

Typing another character usually invalidates the previous speculative request.

---

# 21. Fast and Slow Model Paths

Recommended eventual architecture:

```text
                   editor state
                        │
                        ▼
               fast intervention model
               /         |          \
          NOTHING      simple       escalate
                       action          │
                                       ▼
                               richer model
                                /    |    \
                           rewrite explain warn
```

## 21.1 Fast path

Responsibilities:

- whether to intervene,
- classify likely intent vs executable syntax,
- rank deterministic suggestions,
- decide inline vs menu,
- lightweight history selection.

Potential deployment:

- local small model,
- heuristic classifier,
- hybrid rules + model.

## 21.2 Slow/richer path

Responsibilities:

- natural-language translation,
- whole-command rewrite,
- multi-step command synthesis,
- detailed explanation,
- alternatives,
- risk reasoning.

Provider could be:

- local larger model,
- remote cloud model,
- enterprise endpoint.

---

# 22. Model Provider API

```rust
#[async_trait]
trait SuggestionModel: Send + Sync {
    async fn propose(
        &self,
        request: ProposalRequest,
        cancel: CancellationToken,
    ) -> Result<Proposal>;
}
```

Providers:

```text
LocalLlamaProvider
CloudOpenAIProvider
CloudAnthropicProvider
EnterpriseHttpProvider
DisabledProvider
```

The exact provider set is out of scope for the core architecture.

---

# 23. Privacy Modes

Smartsh should explicitly support:

## 23.1 Local-only

Nothing leaves the machine.

```toml
[ai]
mode = "local"
```

## 23.2 Cloud with redaction

Selected context may be sent after redaction.

```toml
[ai]
mode = "cloud"
send_history = "relevant"
send_cwd = true
send_git_context = true
send_environment = false
```

## 23.3 No-AI mode

Smartsh remains a contextual history/completion frontend.

This is important for reliability and enterprise environments.

---

# 24. Secret Redaction

Before cloud inference, Smartsh should inspect content for likely secrets.

Examples:

- shell variable assignments containing token/key/password names,
- common cloud credential formats,
- bearer tokens,
- private keys,
- credential-bearing URLs,
- sensitive environment-variable names.

Redaction should be conservative.

The safest architecture is:

```text
raw machine context
       │
       ▼
 local context reducer
       │
       ▼
 local secret redactor
       │
       ▼
 cloud provider
```

Cloud providers should never receive raw process environments.

---

# 25. Safety and Command Risk

Smartsh should classify command risk independently from basic LLM generation where possible.

Useful risk classes:

```text
LOW
MODERATE
DESTRUCTIVE
PRIVILEGED
NETWORK_SECURITY_SENSITIVE
UNKNOWN
```

Examples worth warning on:

- `rm -rf`,
- `git reset --hard`,
- `git clean -fd`,
- destructive database operations,
- filesystem format commands,
- `dd` writing to devices,
- recursive permission changes,
- `sudo` commands,
- destructive cloud CLI operations.

The warning system should be contextual, not hysterical.

A user who explicitly typed a command still owns the decision.

---

# 26. Natural Language Detection

Smartsh must avoid a brittle binary parser that declares a line either “English” or “shell”.

Inputs can be mixed.

Examples:

```text
kubectl get pods sorted by creation time
git rebase interactive against main
docker show containers using more than 1GB
grep recursive for foo but ignore node_modules
```

Model/policy output should estimate:

```text
P(explicit_shell)
P(intent_expression)
P(needs_rewrite)
P(needs_explanation)
```

These values guide UX; they should not be treated as mathematical truth.

---

# 27. Deterministic Parser Strategy

Smartsh should parse enough shell syntax to understand:

- command boundaries,
- arguments,
- quotes,
- pipes,
- redirects,
- substitutions,
- logical operators,
- cursor location.

It should NOT initially implement complete execution semantics.

Potential approaches:

1. adopt an existing shell parser,
2. use shell-specific parsing where available,
3. implement a conservative lightweight parser.

The parser must tolerate incomplete input.

Incomplete input is normal during editing.

---

# 28. Session Persistence

Session resilience is a separate subsystem from AI assistance.

Requirement:

> A shell/program running on a remote host should continue running when the SSH transport disappears, and the user should be able to reconnect to the same session.

## 28.1 Phase-one persistence

Use tmux behind the scenes.

Conceptually:

```text
Smartsh client
      │
      ▼
hidden tmux session
      │
      ▼
shell
```

tmux explicitly supports detaching and later reattaching sessions while leaving programs running, including surviving remote connection drops.

Users should not need to learn tmux commands for normal Smartsh operation.

## 28.2 Final persistence architecture

```text
smartsh client
      │
      │ local Unix socket / remote terminal connection
      ▼
smartshd
      │
      ├── session A ─ PTY ─ zsh
      ├── session B ─ PTY ─ bash
      └── session C ─ PTY ─ zsh
```

If the client disappears:

```text
smartshd ─ PTY ─ shell ─ foreground process
```

remains alive.

---

# 29. Reconnect Complexity

Simply preserving a PTY is insufficient for perfect reconnection while a full-screen application is running.

If an SSH link dies inside Vim, reconnecting later requires reconstructing terminal state such as:

- screen cells,
- cursor position,
- attributes,
- alternate-screen state,
- scrolling regions,
- terminal modes.

Therefore implement persistence incrementally.

## Stage A

tmux provides complete persistence.

## Stage B

Smartsh owns PTY lifetime and reconnects primarily at shell prompts.

## Stage C

Smartsh maintains a terminal state model and can redraw running applications after reconnect.

Stage C is effectively a subset of terminal multiplexer functionality.

It is **not** a requirement for the AI MVP.

---

# 30. Scrollback Model

The terminal emulator should remain the primary scrollback owner in ordinary operation.

Smartsh must avoid consuming output into a private screen that the outer terminal cannot see.

For later reconnect support, the daemon may additionally maintain:

- a bounded byte-stream log,
- semantic command/output boundaries,
- terminal screen state.

These are separate from the terminal emulator's own scrollback.

---

# 31. Smartsh Daemon

Long-term daemon responsibilities:

```text
session lifecycle
PTY ownership
shell integration
history persistence
context caching
model-provider management
completion-provider management
local IPC
reconnect support
```

The client handles:

```text
terminal input
line editing
UI rendering
keyboard interaction
```

Some responsibilities may move between client and daemon after latency testing.

---

# 32. IPC Protocol

Local Unix-domain sockets are recommended.

Messages should be versioned.

Example envelope:

```json
{
  "protocol_version": 1,
  "request_id": "uuid",
  "session_id": "uuid",
  "type": "proposal_request",
  "payload": {}
}
```

Message types may include:

```text
HELLO
CREATE_SESSION
ATTACH_SESSION
DETACH_SESSION
INPUT
RESIZE
PROMPT_READY
COMMAND_STARTED
COMMAND_FINISHED
PROPOSAL_REQUEST
PROPOSAL_RESPONSE
HISTORY_QUERY
HISTORY_RESULTS
CONTEXT_UPDATE
ERROR
```

Binary framing (e.g. MessagePack/CBOR/Protobuf) may eventually be useful, but JSON is fine for the first internal protocol.

---

# 33. Configuration

Recommended user config:

```text
~/.config/smartsh/config.toml
```

Example:

```toml
[editor]
mode = "emacs"
show_explanations = true
show_warnings = true

[ai]
mode = "local"
provider = "local"
debounce_ms = 80
auto_rewrite = true

[history]
enabled = true
semantic_search = true
max_context_items = 20

[session]
backend = "tmux"

[privacy]
send_environment = false
redact_secrets = true
```

macOS may additionally support native application-support paths, but consistent XDG-compatible behavior is preferable for CLI users.

---

# 34. Suggested Repository Layout

```text
smartsh/
├── Cargo.toml
├── crates/
│   ├── smartsh-cli/
│   ├── smartsh-editor/
│   ├── smartsh-ui/
│   ├── smartsh-core/
│   ├── smartsh-protocol/
│   ├── smartsh-history/
│   ├── smartsh-context/
│   ├── smartsh-completion/
│   ├── smartsh-ai/
│   ├── smartsh-pty/
│   ├── smartsh-daemon/
│   └── smartsh-shell-integration/
├── integrations/
│   ├── zsh/
│   └── bash/
├── prompts/
│   ├── intervention/
│   ├── rewrite/
│   └── explanation/
├── eval/
│   ├── command-rewrite/
│   ├── risk/
│   ├── intervention/
│   └── history-ranking/
├── docs/
└── tests/
```

A Cargo workspace is recommended.

---

# 35. Observability and Debugging

AI-driven interactive behavior can be frustrating to debug unless Smartsh can explain itself.

Provide a debug command such as:

```bash
smartsh debug last
```

Potential output:

```text
Buffer:
  git interactive rebase on master branch

Decision:
  WHOLE_REWRITE

Context used:
  cwd=/repo
  branch=feature/auth
  default_branch=main
  history_items=7
  completion_candidates=18

Proposal:
  git rebase -i main

Latency:
  context: 3 ms
  fast policy: 11 ms
  model: 63 ms
  render: 1 ms
```

Sensitive values must be redacted.

---

# 36. Performance Requirements

These are target budgets, not guaranteed numbers.

## 36.1 Editing

| Operation | Target |
|---|---:|
| local keystroke echo/edit | < 10 ms |
| deterministic prefix completion | < 20 ms typical |
| history query | < 20 ms typical |
| context lookup from cache | < 10 ms |
| first useful AI hint | < 150 ms desirable |
| explicit richer rewrite | < 500 ms desirable local/cloud dependent |

The primary requirement:

> No AI latency may degrade typing latency.

## 36.2 Startup

Target:

```text
smartsh interactive startup < 100 ms
```

before optional model initialization.

Large local models should be maintained by a persistent service rather than loaded for every shell.

---

# 37. Model Quality Evaluation

A model that writes impressive commands but interrupts at the wrong time is a bad Smartsh model.

Evaluation requires several dimensions.

## 37.1 Intervention precision

Dataset:

```text
buffer + context → should_intervene?
```

False positives are expensive.

## 37.2 Rewrite correctness

Dataset:

```text
intent + machine context → expected command semantics
```

## 37.3 Explanation quality

Check:

- factual correctness,
- mention of destructive effects,
- clarity,
- absence of unsupported claims.

## 37.4 Context selection

Does the system pick the relevant historical command/repository/branch?

## 37.5 Dangerous-command recall

Measure whether risky side effects are surfaced.

## 37.6 Latency

Measure p50/p95/p99.

---

# 38. Initial Evaluation Examples

Include test cases like:

### Natural-language Git

Input:

```text
git interactive rebase on master branch
```

Expected possible output:

```bash
git rebase -i master
```

with explanation about commit-history rewriting.

If repository metadata says the real default branch is `main` and `master` does not exist, expected behavior is to surface that fact and propose `main` rather than blindly copying the phrase.

### Docker

Input:

```text
remove all stopped docker containers
```

Expected:

```bash
docker container prune
```

with note that Docker requests confirmation unless forced.

### Mixed syntax

Input:

```text
kubectl get pods sorted by creation time
```

Expected:

```bash
kubectl get pods --sort-by=.metadata.creationTimestamp
```

### Valid explicit command

Input:

```bash
git status
```

Expected default:

```text
NOTHING
```

### Dangerous explicit command

Input:

```bash
git reset --hard origin/main
```

Expected:

- command remains unchanged,
- concise destructive-operation warning may appear.

### History-sensitive command

Historical context contains repeated:

```bash
ssh deploy@staging.internal
```

Input:

```text
ssh dep
```

Expected:

- history-derived completion should rank highly,
- no LLM hallucination of a new host.

---

# 39. Testing Strategy

## 39.1 Unit tests

Test:

- text edits,
- buffer revisions,
- history ranking,
- redaction,
- proposal validation,
- command-risk rules,
- protocol serialization,
- context reducers.

## 39.2 Property tests

Useful for:

- Unicode cursor/edit behavior,
- arbitrary incomplete quoting,
- text replacement boundaries,
- escape-sequence handling.

## 39.3 PTY integration tests

Run real:

- Bash,
- Zsh,
- `cat`,
- `less`,
- Vim/Neovim if available,
- SSH client in a controlled environment.

Verify:

- input forwarding,
- terminal resize,
- Ctrl-C,
- Ctrl-Z,
- process groups,
- exit codes,
- prompt transitions.

## 39.4 Terminal compatibility matrix

At minimum:

### macOS

- Terminal.app
- iTerm2
- WezTerm

### Linux

- GNOME Terminal
- Konsole
- Kitty
- WezTerm
- Alacritty

SSH scenarios should be tested separately.

## 39.5 Shell compatibility

Initial:

- Zsh
- Bash

Later:

- Fish compatibility mode if desirable,
- Nushell integration,
- other shells via adapters.

---

# 40. Security Threat Model

Consider at least:

## 40.1 Prompt injection from local files

If Smartsh reads repository content for context, malicious text could attempt to manipulate the LLM.

Mitigation:

- keep retrieved data explicitly labeled as untrusted data,
- prefer structured context,
- minimize arbitrary file ingestion.

## 40.2 Secret leakage

Mitigation:

- local redaction,
- no environment dump,
- explicit privacy modes,
- provider audit log.

## 40.3 AI hallucinated flags

Mitigation:

- ground known command syntax in deterministic completion/spec providers,
- validate flags when practical,
- show command before execution.

## 40.4 Destructive generated commands

Mitigation:

- warnings,
- no auto-execution,
- risk classification.

## 40.5 History database sensitivity

History often contains secrets.

Mitigation options:

- restrictive file permissions,
- optional encryption,
- redaction rules,
- local-only default,
- easy deletion/export controls.

---

# 41. Failure Modes

Smartsh must degrade gracefully.

## Model unavailable

Continue with:

- editing,
- history,
- deterministic completion.

## Completion provider crashes

Disable provider for the current request and continue.

## Daemon unavailable

Client should offer to create a new daemon/session automatically.

## tmux unavailable during MVP

Either:

- install is documented as a dependency for resilience, or
- run without persistence with a visible degraded-state indicator.

## Shell integration fails

Fall back to conservative prompt detection or plain pass-through mode.

## Unknown terminal behavior

Prefer minimal escape sequences over aggressive rendering.

---

# 42. Project Phases

---

## Phase 0 — UX Validation Prototype

**Goal:** Validate the core idea before building a PTY/session stack.

Recommended implementation:

```text
Zsh plugin
    │
    └── Smartsh local service
          ├── history
          ├── LLM
          └── completion/context
```

### Deliverables

- capture current ZLE buffer,
- send contextual request to Smartsh service,
- inline suggestion,
- whole-line rewrite,
- explanation,
- warning,
- contextual history,
- deterministic completion integration,
- no mouse capture.

### Key experiment

Is proactive intervention genuinely better than explicit AI invocation?

Measure:

- suggestion acceptance rate,
- dismissal rate,
- interventions per command,
- percentage of accepted whole-line rewrites,
- time saved relative to manual lookup.

### Exit criteria

Proceed if:

1. users accept a meaningful fraction of proposals,
2. false-positive interventions can be kept low,
3. whole-command rewrite is demonstrably useful,
4. model latency does not feel intrusive.

---

## Phase 1 — Independent Smartsh Editor

**Goal:** Own the command-editing experience while retaining Zsh/Bash execution.

Architecture:

```text
terminal
   │
smartsh editor
   │
PTY
   │
zsh/bash
```

### Deliverables

- Rust editor,
- arbitrary text edit model,
- ghost suggestions,
- menu rendering,
- whole-line rewrite diff,
- explanation renderer,
- deterministic completion providers,
- history database,
- Bash/Zsh shell integration,
- transparent foreground-program passthrough.

### Recommended starting point

Prototype on Reedline and decide based on measured limitations whether to:

- contribute upstream,
- fork,
- replace specific layers,
- build a smaller custom editor.

### Exit criteria

- interactive Vim works normally,
- Ctrl-C/Ctrl-Z semantics work,
- scrolling and mouse selection remain terminal-native,
- Bash and Zsh basic workflows pass compatibility tests.

---

## Phase 2 — Contextual Intelligence

**Goal:** Make Smartsh materially smarter than generic command generation.

### Deliverables

- Git context provider,
- contextual history scoring,
- command-schema provider,
- Carapace evaluation/integration,
- local command help ingestion,
- structured proposal API,
- warnings,
- local/cloud provider abstraction,
- cancellation/versioning,
- model evaluation suite.

### Exit criteria

For supported CLI families, Smartsh should outperform a context-free prompt-to-command model on:

- correctness,
- relevant flag selection,
- branch/host/path selection,
- explanation accuracy.

---

## Phase 3 — Seamless Persistent Sessions Using tmux

**Goal:** Meet SSH-hangup resilience without prematurely reimplementing a multiplexer.

### Deliverables

- hidden named Smartsh tmux sessions,
- auto-create/attach,
- session enumeration,
- transparent reconnect,
- session metadata database,
- terminal resize synchronization.

### UX target

A user should be able to:

```text
ssh server
smartsh
```

lose connectivity, reconnect, run:

```text
smartsh
```

and return to the previous session.

Users should not need to know or care that tmux is providing persistence.

---

## Phase 4 — Native Session Daemon

**Goal:** Remove the architectural dependency on tmux.

### Deliverables

- persistent Smartsh PTY daemon,
- session detach/attach,
- Unix-socket IPC,
- multiple sessions,
- session cleanup policy,
- shell/process lifetime ownership,
- reconnect at shell prompt.

### Exit criteria

Normal shell sessions survive client death and SSH disconnect without tmux.

---

## Phase 5 — Full Screen-State Reconnect

**Goal:** Reliably reconnect while arbitrary full-screen programs are active.

### Deliverables

- VT terminal state parser/model,
- current screen snapshot,
- terminal mode tracking,
- redraw on attach,
- alternate-screen support,
- scroll-region handling.

This is the phase in which Smartsh intentionally implements selected terminal-multiplexer behavior.

It should not be pulled into the MVP.

---

# 43. Suggested 90-Day Engineering Plan

Assumes a small, highly capable engineering team.

## Weeks 1–2: UX Spike

- build Zsh proof of concept,
- capture/edit line buffer,
- connect local LLM/cloud model abstraction,
- implement proposal JSON contract,
- implement whole-line rewrite,
- implement explanation display,
- collect 100–300 representative command-intent examples.

**Output:** interactive demo.

## Weeks 3–4: Context and History

- SQLite history,
- cwd/session/exit status,
- Git provider,
- basic ranking,
- cancellation/debounce,
- deterministic candidate source.

**Output:** contextual demo that outperforms raw prompt-to-command.

## Weeks 5–6: Evaluation and Intervention Policy

- build intervention dataset,
- NOTHING-first policy,
- risk warning rules,
- latency instrumentation,
- acceptance/dismiss metrics.

**Output:** decision on whether the UX thesis is validated.

## Weeks 7–9: Rust Editor Prototype

- Reedline integration/fork spike,
- arbitrary replacements,
- ghost text,
- transient menus,
- explanation pane,
- bracketed paste,
- terminal resize,
- no mouse capture.

**Output:** first standalone Smartsh editor.

## Weeks 10–11: PTY + Shell Integration

- spawn Zsh/Bash under PTY,
- transparent passthrough,
- prompt-ready markers,
- Ctrl-C/job-control tests,
- Vim/less/top compatibility.

**Output:** Smartsh usable as a daily shell frontend.

## Week 12: Persistence + Packaging

- tmux-backed automatic session attach,
- installer,
- config,
- crash/degraded-mode handling,
- release packaging,
- internal alpha.

**Output:** v0.1 alpha.

---

# 44. Team Breakdown

For an initial team of 3–5 engineers:

## Terminal/systems engineer

Owns:

- PTY,
- process groups,
- shell integration,
- input routing,
- daemon,
- reconnect.

## Editor/UX engineer

Owns:

- line editing,
- rendering,
- menus,
- diff UX,
- keybindings,
- compatibility.

## AI/context engineer

Owns:

- intervention model,
- prompts/contracts,
- history ranking,
- context selection,
- command grounding,
- evals.

## Optional additional roles

- product/design engineer,
- security/privacy engineer,
- CLI ecosystem/integrations engineer.

A strong Rust engineer can initially cover both editor and systems work.

---

# 45. Release Strategy

## v0.0.x

Developer-only Zsh plugin experiments.

## v0.1

Standalone editor + Zsh, experimental Bash, cloud/local model provider, tmux-backed resilience.

## v0.2

Improved context, provider ecosystem, polished history, better command explanations.

## v0.3

Native daemon experimental.

## v1.0 criteria

Do not call the product 1.0 until:

- Bash/Zsh compatibility is strong,
- normal terminal applications behave predictably,
- false AI interventions are low,
- generated commands are visibly staged,
- local privacy mode is robust,
- reconnect behavior is reliable,
- model failure never makes the shell unusable.

---

# 46. Success Metrics

Recommended product metrics:

## Interaction quality

- proposal acceptance rate,
- acceptance rate by action type,
- dismissal rate,
- undo-after-accept rate,
- commands executed unchanged,
- explanations explicitly expanded.

## Efficiency

- characters saved,
- time from initial typing to command execution,
- reduction in `man` / `--help` lookups,
- successful history reuse.

## Intelligence

- rewrite semantic correctness,
- contextually correct branch/path/host choice,
- dangerous-command warning recall,
- false warning rate.

## Performance

- keystroke latency,
- completion latency,
- AI response latency,
- startup latency,
- memory footprint.

## Reliability

- shell crashes caused by Smartsh,
- passthrough failures,
- reconnect success rate,
- corrupted history incidents.

---

# 47. Major Technical Risks

## Risk 1: Fighting shell job control

PTY/process-group semantics are subtle.

**Mitigation:** do not attempt to redesign them; pass execution to a real shell and build extensive PTY integration tests.

## Risk 2: AI is too noisy

Proactive assistance can become irritating.

**Mitigation:** optimize intervention precision, make NOTHING a first-class decision, use conservative thresholds.

## Risk 3: Slow AI makes the shell feel slow

**Mitigation:** asynchronous/cancellable model calls; deterministic fast path; persistent local model service.

## Risk 4: Rich UI damages scrollback

**Mitigation:** never permanently enter alternate-screen mode; keep mouse uncaptured; test repaint behavior across terminals.

## Risk 5: LLM invents invalid command syntax

**Mitigation:** structured completion grounding, local `--help`/spec data, candidate validation.

## Risk 6: Secret leakage

**Mitigation:** local-first architecture, strict context reducer, secret redaction, environment allowlist.

## Risk 7: Scope expands into a terminal emulator/tmux clone too early

**Mitigation:** explicitly defer native screen-state reconstruction until the product thesis is validated.

---

# 48. Open Engineering Decisions

These should be resolved experimentally rather than by architecture debate alone.

1. Reedline fork vs custom editor.
2. Shell parser library.
3. Exact semantic prompt/control protocol.
4. Best source for command specifications.
5. Whether Carapace should be embedded, invoked, or merely supported.
6. Local embedding usefulness for history.
7. Fast-model architecture: heuristic classifier vs small LLM.
8. Default cloud/local mode.
9. How often explanations should be visible automatically.
10. How much warning UX is useful without becoming intrusive.
11. Whether Smartsh should maintain its own scrollback metadata before native persistence.
12. How to support nested SSH sessions elegantly.
13. Whether one daemon manages all local sessions or each login owns one daemon.
14. How remote Smartsh installations discover/reuse persistent sessions.
15. Whether semantic terminal markers should use OSC 133/633 compatibility plus private extensions.

---

# 49. Recommended Immediate Decisions

The following choices are recommended for the first implementation:

```text
Language:            Rust
Initial shell:       Zsh
Second shell:        Bash
Terminal emulator:   existing user's terminal
Mouse capture:       NEVER in normal Smartsh mode
History:             SQLite
Editor prototype:    ZLE first, then Reedline-based standalone spike
Async runtime:       Tokio
Terminal input:      Crossterm
PTY:                 portable-pty
Persistence MVP:     hidden tmux session
Final persistence:   Smartsh daemon
AI contract:         structured Proposal object
Execution policy:    visible staging before LLM-generated execution
Model support:       provider abstraction
Privacy:             local-first capable
```

---

# 50. MVP Definition

The smallest MVP that tests the differentiated product is:

> **A Zsh-integrated Smartsh assistant that watches the current editable buffer and recent contextual history, proactively decides when assistance is useful, can replace the entire buffer from natural-language intent, and always explains generated commands before they are executed.**

The MVP DOES need:

- proactive intervention,
- whole-command rewrite,
- explanations,
- relevant history,
- deterministic completion grounding,
- local/cloud model abstraction,
- no mouse capture.

The MVP DOES NOT need:

- new terminal emulator,
- new shell interpreter,
- native PTY daemon,
- native reconnect,
- full terminal screen model,
- tmux replacement.

This scope should validate the main product thesis with dramatically less engineering risk.

---

# 51. Long-Term Product Architecture

The eventual product can become:

```text
┌───────────────────────────────────────────────────────────────┐
│ User's terminal emulator                                      │
│ Native scrolling, selection, copy/paste                       │
└───────────────────────────┬───────────────────────────────────┘
                            │
                            ▼
┌───────────────────────────────────────────────────────────────┐
│ Smartsh interactive client                                    │
│                                                               │
│ editor ─ suggestions ─ explanations ─ warnings ─ history UI   │
└───────────────────────────┬───────────────────────────────────┘
                            │
                            ▼
┌───────────────────────────────────────────────────────────────┐
│ Smartsh daemon                                                │
│                                                               │
│ PTY sessions                                                  │
│ contextual history                                            │
│ command knowledge                                             │
│ Git/filesystem/context providers                              │
│ local/cloud model providers                                   │
│ session persistence                                           │
│ terminal state for reconnect                                  │
└───────────────────────────┬───────────────────────────────────┘
                            │
                            ▼
                 ┌──────────────────────┐
                 │ Bash / Zsh           │
                 │ unchanged semantics  │
                 └──────────┬───────────┘
                            │
                            ▼
                   Unix tools and programs
```

This architecture preserves compatibility while making the interactive shell experience substantially more intelligent.

---

# 52. Core Architectural Principle

Smartsh should own **intent and interaction**, not Unix semantics.

The terminal should continue to own terminal presentation.

Bash/Zsh should continue to own shell execution.

Smartsh should own the space between them:

```text
"What is the user trying to do?"
"What does their history suggest?"
"What options are actually available?"
"Would helping right now be useful?"
"Should I complete, rewrite, explain, warn, or remain silent?"
```

That boundary is both technically practical and the strongest expression of the product's differentiation.

---

# 53. References / Current Technology Notes

The implementation recommendations above were checked against current project documentation on 2026-09-07.

- Reedline: https://docs.rs/reedline/latest/reedline/
- Crossterm: https://docs.rs/crossterm/latest/crossterm/
- portable-pty: https://docs.rs/sysprims-pty/latest/portable_pty/
- tmux Getting Started: https://github.com/tmux/tmux/wiki/Getting-Started
- Atuin documentation: https://docs.atuin.sh/
- Carapace bridge documentation: https://carapace-sh.github.io/carapace-bin/spec/bridge.html

These dependencies are implementation candidates, not part of the Smartsh product contract. Re-evaluate versions and API suitability when implementation begins.
