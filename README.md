# CodexHub

CodexHub is a cross-platform multi `CODEX_HOME` profile manager for the OpenAI Codex CLI on Linux, macOS, and Windows. It is not an `auth.json` switcher, not an account pool, and not a quota bypass tool.

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
codexhub activate personal
codexhub run personal
```

## Multiple Accounts

```bash
codexhub create work
codexhub login work

codexhub create personal
codexhub login personal
```

Each profile has its own `auth.json`, refresh token, session history, state databases, logs, and config. Login and token refresh are normally handled by the official `codex` CLI. CodexHub can also import a single account from an explicit sub2 JSON export into a new isolated profile.

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

[Account List Screenshot]

[Doctor Screenshot]

The account list loads immediately, then refreshes each logged-in profile's Codex account status in the background through the official Codex app-server API. It shows plan type, remaining 5h and 7day quota percentages, and the membership expiration date when available. Profiles are sorted from earliest membership expiration to latest membership expiration, with unknown expirations last. The right side of the main view shows the selected account details, quota, storage, and path.

The TUI uses the terminal's light/dark preference when it can detect one. Press `t` to cycle the saved theme preference through `auto`, `light`, and `dark`. You can also force a theme for one run with `CODEXHUB_THEME=light` or `CODEXHUB_THEME=dark`.

Press `n` to add an account. The TUI can log in a new account with device-code or web login, import the current `~/.codex`, or import a sub2 JSON export. Press `Enter` from the main account list to activate the selected account as the current `CODEX_HOME` for tools launched outside CodexHub. Press `l` to relogin the selected account, `o` to open Codex with the selected account, and `r` to refresh account status. CodexHub writes `~/.codexhub/current.env`, `~/.codexhub/activate.sh` on Linux/macOS or `activate.ps1` on Windows, and `~/.codexhub/current_profile`. On Linux it also writes a user environment file under `~/.config/environment.d/`; on Linux/macOS/Windows it best-effort publishes the value to the current user desktop environment. Restart Codex Desktop after switching because already-running apps cannot receive environment changes from CodexHub.

CodexHub checks GitHub for updates in the background when the TUI starts. If the remote repository has a newer commit, it shows an update prompt. Press `Enter` or `y` to run `git pull --ff-only` and `cargo install --path .`, or press `n`/`Esc` to skip.

Press `h` from the account list to show resume sessions across all profiles and the default `~/.codex` home. The history screen loads in the background and supports scrolling through the merged session list. By default, it shows sessions whose CWD matches the directory where CodexHub was started; press `a` to toggle between current-path sessions and all paths. CodexHub reads the same persisted history used by Codex resume through the official app-server `thread/list` API. The right side previews the selected session. Press `Enter` on a row to run `codex resume` with that session's original `CODEX_HOME`. Press `q` from history or doctor to return to the main account list; press `q` again from the main view to quit.

If one profile runs out of quota while working on a session, open the history screen, select that session, press `c`, choose the target profile from the popup list, and press `Enter`. CodexHub copies the selected session file and session index entry into the target profile's isolated `CODEX_HOME`, then runs `codex resume` from that target profile.

## Shared Cache

Only low-risk cache-like paths can be shared, and sharing is implemented with symlinks into `~/.codexhub/shared`. On Windows, creating symlinks may require Developer Mode or appropriate privileges.

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

CodexHub therefore never shares or overwrites `auth.json` between profiles. Use:

```bash
codexhub login work
codexhub login personal
```

This runs the official `codex login` with the correct `CODEX_HOME`.

For one-time migration from a sub2 JSON export, use `codexhub import-sub2 <json> [name]`. This creates a new profile directory and writes a fresh Codex-style `auth.json` for that profile only.

## Why Not Share Sessions or History

`sessions/`, `history.jsonl`, and `session_index.jsonl` describe local conversations and resume state. Sharing them across accounts risks cross-account resume confusion and accidental data mixing. CodexHub treats them as isolated profile state and `doctor` reports shared sensitive files as errors.

## Why No Rotation or Private Quota API

CodexHub does not implement account rotation, automatic switching to avoid limits, manual refresh-token calls, or private API quota checks. It starts the official Codex CLI with a selected `CODEX_HOME` and can explicitly activate one profile as the current user environment for other Codex clients.

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
codexhub import-default [name]
codexhub import-sub2 <json> [name]
codexhub login <name>
codexhub activate <name>
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

The most important invariant is simple: CodexHub never becomes an auth manager. It manages isolated `CODEX_HOME` directories and delegates login, token refresh, and Codex behavior to the official CLI. Activating a profile changes which isolated home future Codex clients see; it does not copy or merge credentials.

## Import Existing `~/.codex`

To import your current default Codex home without logging in again:

```bash
codexhub import-default
```

When no name is provided, CodexHub derives the profile name from the email address in `~/.codex/auth.json`. You can also pass a name explicitly:

```bash
codexhub import-default personal
```

The import copies the whole `~/.codex` home into `~/.codexhub/profiles/<name>/`, skipping runtime `tmp/` files. Do not import the same default account into multiple profiles.

## Import sub2 JSON

To import one OpenAI account from a sub2 JSON export:

```bash
codexhub import-sub2 accounts.json
```

CodexHub derives the profile name from the account email address. You can pass an explicit name:

```bash
codexhub import-sub2 accounts.json work
```

The import creates `~/.codexhub/profiles/<name>/`, writes a Codex-compatible `auth.json`, creates `sessions/`, and copies `~/.codex/config.toml` when it exists. In the TUI, press `n`, choose `Import JSON`, and enter the JSON path.
