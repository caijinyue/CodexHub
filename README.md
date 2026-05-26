# CodexHub

CodexHub is a multi `CODEX_HOME` profile manager for the OpenAI Codex CLI. It is not an `auth.json` switcher, not an account pool, and not a quota bypass tool.

Codex CLI stores local state under `~/.codex` by default. It also supports `CODEX_HOME=/some/path codex`, which lets each account run with a physically separate home directory. CodexHub uses that supported boundary:

```bash
CODEX_HOME="$HOME/.codexhub/profiles/work" codex
CODEX_HOME="$HOME/.codexhub/profiles/personal" codex
```

## Install

```bash
cargo install --path .
```

or:

```bash
cargo build --release
```

## Quick Start

```bash
codexhub init
codexhub create personal
codexhub login personal
codexhub run personal
```

## Multiple Accounts

```bash
codexhub create work
codexhub login work

codexhub create personal
codexhub login personal
```

Each profile has its own `auth.json`, refresh token, session history, state databases, logs, and config. Login and token refresh are always handled by the official `codex` CLI.

## Concurrent Runs

Terminal 1:

```bash
codexhub run work
```

Terminal 2:

```bash
codexhub run personal
```

## Exec

```bash
codexhub exec work -- "检查这个项目"
```

Additional Codex flags are passed through:

```bash
codexhub exec work -- --sandbox danger-full-access "修复测试"
```

## TUI

```bash
codexhub tui
```

or just:

```bash
codexhub
```

[Profile List Screenshot]

[Profile Detail Screenshot]

[Doctor Screenshot]

## Shared Cache

Only low-risk cache-like paths can be shared, and sharing is implemented with symlinks into `~/.codexhub/shared`:

```bash
codexhub share-cache work
codexhub share-cache personal
```

Allowed shared paths:

```text
plugins/
vendor_imports/
skills/
rules/
models_cache.json
computer-use/
cache/
```

Sensitive account and session files are never shared by CodexHub:

```text
auth.json
installation_id
sessions/
history.jsonl
session_index.jsonl
state_*.sqlite
goals_*.sqlite
logs_*.sqlite
.credentials.json
```

## Why Not Copy `auth.json`

`auth.json` contains local login credentials and refresh tokens. Refresh tokens can rotate. Copying one `auth.json` into multiple profiles can leave one profile with a stale token after another profile refreshes it.

CodexHub therefore never copies, reads, prints, shares, or overwrites `auth.json`. Use:

```bash
codexhub login work
codexhub login personal
```

This runs the official `codex login` with the correct `CODEX_HOME`.

## Why Not Share Sessions or History

`sessions/`, `history.jsonl`, and `session_index.jsonl` describe local conversations and resume state. Sharing them across accounts risks cross-account resume confusion and accidental data mixing. CodexHub treats them as isolated profile state and `doctor` reports shared sensitive files as errors.

## Why No Rotation or Private Quota API

CodexHub does not implement account rotation, automatic switching to avoid limits, manual refresh-token calls, or private API quota checks. It only starts the official Codex CLI with a selected `CODEX_HOME`.

## Difference From Auth Switchers

Tools such as `codex-auth`, `codex-multi-auth`, or generic `auth.json` switchers commonly manage credentials by copying or swapping auth files. CodexHub manages whole physical Codex homes instead:

```text
~/.codexhub/profiles/work/
~/.codexhub/profiles/personal/
```

The boundary is the directory, not a credential file.

## Commands

```bash
codexhub init
codexhub create <name> [--copy-config]
codexhub login <name>
codexhub run <name> -- [codex args...]
codexhub exec <name> -- [codex exec args...]
codexhub shell <name>
codexhub path <name>
codexhub list
codexhub doctor [--allow-auth-symlink]
codexhub share-cache <name>
codexhub unshare-cache <name> [--restore-backup|--keep-empty]
codexhub delete <name>
codexhub tui
```

## Architecture

CodexHub is split into narrow modules:

- `cli`: command parsing and routing.
- `config`: `~/.codexhub` path resolution and config file creation.
- `profile`: profile lifecycle and metadata.
- `doctor`: Codex binary checks and profile isolation checks.
- `process`: official Codex CLI execution with inherited stdio.
- `shared`: safe shared-cache symlink management.
- `size`: file tree size and human-readable formatting.
- `shell`: interactive subshell with `CODEX_HOME`.
- `tui`: ratatui/crossterm interface, input popups, doctor view, and external command handoff.

The most important invariant is simple: CodexHub never becomes an auth manager. It manages isolated `CODEX_HOME` directories and delegates login, token refresh, and Codex behavior to the official CLI.
