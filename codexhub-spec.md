# CodexHub 需求文档

## 项目定位

实现一个开源级别的 OpenAI Codex CLI 多账号管理工具：

```bash
codexhub
```

CodexHub 的目标是提供类似 Claude Code Router / CCManager 的管理体验，但底层必须坚持：

```text
物理隔离 CODEX_HOME profile，而不是 auth.json 切换器。
```

CodexHub 不是 auth manager，也不是账号池 rotation 工具。

它是：

```text
multi CODEX_HOME profile manager for OpenAI Codex CLI
```

---

## 核心目标

CodexHub 必须支持：

- 多个 Codex 账号长期共存
- 每个账号独立 `CODEX_HOME`
- 每个账号独立 `auth.json`
- 每个账号独立 refresh token
- 每个账号独立 session / history / state
- 支持多个账号并发运行
- 支持 CLI
- 支持 TUI
- 支持可选共享低风险大缓存目录
- 支持 doctor 安全检查
- 支持 macOS 和 Linux

CodexHub 不能实现：

- 复制 `auth.json`
- 覆盖 `~/.codex/auth.json`
- 共享 `auth.json`
- 默认允许 `auth.json` symlink
- 账号池自动 rotation
- 自动切换账号绕限制
- 私有 API 额度查询
- 手动刷新 refresh token
- 任何规避 OpenAI 限制的逻辑

---

## 背景说明

OpenAI Codex CLI 默认使用：

```text
~/.codex/
```

作为状态目录。

Codex 支持通过环境变量指定不同状态目录：

```bash
CODEX_HOME=/some/path codex
```

因此 CodexHub 的核心实现方式应该是：

```bash
CODEX_HOME="$HOME/.codexhub/profiles/work" codex
CODEX_HOME="$HOME/.codexhub/profiles/personal" codex
```

每个 profile 都是一个完整独立的 Codex home。

---

## 为什么不能复制 auth.json

`auth.json` 包含 Codex / OpenAI 的本地登录凭证和 refresh token。

refresh token 可能是 rotating / single-use。

如果把同一个 `auth.json` 复制到多个 profile：

```text
profile-a/auth.json
profile-b/auth.json
```

当 profile-a 刷新 token 后，profile-b 中的旧 refresh token 可能失效。

因此 CodexHub 必须遵守：

```text
不要复制 auth.json
不要共享 auth.json
不要覆盖 ~/.codex/auth.json
不要手动调用 OAuth refresh API
登录和刷新都交给官方 Codex CLI 自己处理
```

正确方式：

```bash
CODEX_HOME="$HOME/.codexhub/profiles/work" codex login
CODEX_HOME="$HOME/.codexhub/profiles/personal" codex login
```

让每个 profile 独立登录，生成自己的 `auth.json`。

---

## 当前 Codex 目录结构参考

用户当前 `~/.codex` 示例：

```text
AGENTS.md
ambient-suggestions/
auth.json
cache/
computer-use/
config.toml
goals_1.sqlite
history.jsonl
installation_id
log/
logs_2.sqlite
logs_2.sqlite-shm
logs_2.sqlite-wal
memories/
models_cache.json
plugins/
rules/
session_index.jsonl
sessions/
shell_snapshots/
skills/
sqlite/
state_5.sqlite
tmp/
vendor_imports/
version.json
```

---

## 必须隔离的内容

每个 profile 必须独立拥有：

```text
auth.json
installation_id
config.toml
history.jsonl
session_index.jsonl
sessions/
state_*.sqlite
goals_*.sqlite
logs_*.sqlite
.credentials.json
```

说明：

- `auth.json`：账号凭证，必须独立
- `installation_id`：本地安装身份，建议独立
- `sessions/`：会话数据，必须独立，避免 resume 串号
- `history.jsonl`：历史记录，必须独立
- `session_index.jsonl`：session 索引，必须独立
- `state_*.sqlite`：本地状态，必须独立
- `goals_*.sqlite`：任务状态，必须独立
- `logs_*.sqlite`：日志数据库，建议独立，避免账号信息混杂

---

## 可以共享的低风险缓存

CodexHub 可以支持可选共享以下内容：

```text
plugins/
vendor_imports/
skills/
rules/
models_cache.json
computer-use/
cache/
```

共享方式必须使用 symlink：

```text
~/.codexhub/profiles/work/plugins -> ~/.codexhub/shared/plugins
```

共享前必须备份 profile 内原文件或目录：

```text
plugins.bak.<timestamp>
vendor_imports.bak.<timestamp>
computer-use.bak.<timestamp>
```

---

## CodexHub 目录结构

CodexHub 统一使用：

```text
~/.codexhub/
```

目录结构：

```text
~/.codexhub/
├── config.toml
├── profiles/
│   ├── work/
│   ├── personal/
│   └── client-a/
├── shared/
│   ├── plugins/
│   ├── vendor_imports/
│   ├── skills/
│   ├── rules/
│   ├── models_cache.json
│   ├── computer-use/
│   └── cache/
├── backups/
└── logs/
```

每个 profile 都是一个完整独立的 `CODEX_HOME`：

```text
~/.codexhub/profiles/work/
├── auth.json
├── config.toml
├── history.jsonl
├── session_index.jsonl
├── sessions/
├── state_5.sqlite
├── goals_1.sqlite
├── logs_2.sqlite
└── ...
```

---

# CLI 命令设计

## 1. 初始化

```bash
codexhub init
```

创建：

```text
~/.codexhub/
~/.codexhub/config.toml
~/.codexhub/profiles/
~/.codexhub/shared/
~/.codexhub/backups/
~/.codexhub/logs/
```

---

## 2. 创建 Profile

```bash
codexhub create <name>
```

示例：

```bash
codexhub create personal
codexhub create work
```

行为：

- 创建 `~/.codexhub/profiles/<name>/`
- 初始化必要目录
- 默认不复制任何 `auth.json`
- 默认不复制 session/history/state
- 可选复制默认配置

参数：

```bash
--copy-config
```

如果指定：

```bash
codexhub create work --copy-config
```

则可以复制：

```text
~/.codex/config.toml -> ~/.codexhub/profiles/work/config.toml
```

只允许复制 `config.toml`，不能复制 `auth.json`。

---

## 3. 登录 Profile

```bash
codexhub login <name>
```

执行：

```bash
CODEX_HOME="$HOME/.codexhub/profiles/<name>" codex login
```

要求：

- 必须调用官方 `codex login`
- 继承 stdin / stdout / stderr
- 让官方 Codex 自己生成 `auth.json`
- CodexHub 本身不要读取或显示 `auth.json` 内容

---

## 4. 运行 Codex

```bash
codexhub run <name>
```

执行：

```bash
CODEX_HOME="$HOME/.codexhub/profiles/<name>" codex
```

也支持透传参数：

```bash
codexhub run work -- --model gpt-5.1-codex
```

实际执行：

```bash
CODEX_HOME="$HOME/.codexhub/profiles/work" codex --model gpt-5.1-codex
```

---

## 5. 执行 Codex Exec

```bash
codexhub exec <name> -- "<prompt>"
```

示例：

```bash
codexhub exec work -- "检查这个 Rust 项目并修复编译错误"
```

执行：

```bash
CODEX_HOME="$HOME/.codexhub/profiles/work" codex exec "检查这个 Rust 项目并修复编译错误"
```

也支持额外参数：

```bash
codexhub exec work -- --sandbox danger-full-access "修复测试"
```

---

## 6. 打开 Profile Shell

```bash
codexhub shell <name>
```

行为：

- 打开一个子 shell
- 设置 `CODEX_HOME`
- 修改 prompt 显示当前 profile

示例 prompt：

```text
(codex:work) user@host project %
```

退出 shell 后恢复原环境。

---

## 7. 输出 Profile 路径

```bash
codexhub path <name>
```

输出：

```text
/Users/<user>/.codexhub/profiles/work
```

方便脚本使用：

```bash
cd "$(codexhub path work)"
```

---

## 8. 列出 Profiles

```bash
codexhub list
```

显示字段：

```text
Name
Path
Logged In
Auth Mtime
Sessions Size
Logs Size
Total Size
Shared Cache
```

示例：

```text
NAME       LOGIN  AUTH MTIME           SESSIONS  LOGS   TOTAL  SHARED
personal   yes    2026-05-27 10:12     34M       88M    140M   yes
work       yes    2026-05-26 22:08     12M       20M    55M    no
client-a   no     -                    0B        0B     4K     no
```

---

## 9. Doctor 检查

```bash
codexhub doctor
```

检查项目：

### Codex 环境

- `codex` 是否安装
- `codex --version`
- 是否能正常传递 `CODEX_HOME`

### Profile 检查

对每个 profile：

- profile 目录是否存在
- `auth.json` 是否存在
- `auth.json` 是否 symlink
- `auth.json` inode
- `auth.json` mtime
- `config.toml` 是否存在
- `sessions/` 是否存在
- `history.jsonl` 是否存在
- `session_index.jsonl` 是否存在
- 是否存在损坏 symlink

### 安全检查

必须检测：

- 是否两个 profile 的 `auth.json` 是同一个 inode
- 是否两个 profile 的 `auth.json` 指向同一个真实路径
- 是否有 profile 的 `auth.json` 是 symlink
- 是否误共享 `sessions/`
- 是否误共享 `history.jsonl`
- 是否误共享 `session_index.jsonl`
- 是否误共享 `state_*.sqlite`
- 是否误共享 `goals_*.sqlite`
- 是否误共享 `logs_*.sqlite`

结果级别：

```text
OK
WARN
ERROR
```

如果发现共享 auth：

```text
ERROR: profiles "work" and "personal" share the same auth.json inode.
```

---

## 10. 共享缓存

```bash
codexhub share-cache <name>
```

行为：

- 为指定 profile 共享低风险缓存
- 备份原有目录
- 创建 symlink

允许共享：

```text
plugins/
vendor_imports/
skills/
rules/
models_cache.json
computer-use/
cache/
```

禁止共享：

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

---

## 11. 取消共享缓存

```bash
codexhub unshare-cache <name>
```

行为：

- 移除共享 symlink
- 可选择恢复最近备份

参数建议：

```bash
--restore-backup
--keep-empty
```

---

## 12. 删除 Profile

```bash
codexhub delete <name>
```

要求：

- 必须二次确认
- 必须输入 profile 名称
- 默认不删除 shared cache

示例：

```text
Type profile name "work" to confirm deletion:
```

---

# TUI 要求

必须实现：

```bash
codexhub tui
```

技术栈：

```text
ratatui
crossterm
```

也可以让单独执行：

```bash
codexhub
```

默认进入 TUI。

---

## TUI 页面 1：Profile List

显示所有 profile：

```text
Name
Logged In
Auth Age
Sessions
Logs
Total
Shared
Path
```

快捷键：

```text
↑ / ↓        移动
j / k        移动
Enter        进入详情

n            新建 profile
d            删除 profile

l            登录当前 profile
r            运行当前 profile
e            输入 prompt 并执行 codex exec

s            share-cache
u            unshare-cache

D            doctor
q            退出
```

---

## TUI 页面 2：Profile Detail

显示：

```text
Profile Name
Profile Path
Auth Exists
Auth Mtime
Auth Is Symlink
Auth Inode
Config Exists
History Size
Session Index Size
Sessions Size
Logs Size
Total Size
Shared Cache Status
Broken Symlinks
```

快捷键：

```text
b            返回列表
l            登录
r            运行 Codex
e            Exec Prompt
s            Share Cache
u            Unshare Cache
D            Doctor
q            退出
```

---

## TUI 页面 3：Doctor

显示检查结果树：

```text
Codex
 ├── binary: OK
 ├── version: OK

Profiles
 ├── work
 │   ├── auth exists: OK
 │   ├── auth symlink: OK
 │   ├── auth inode unique: OK
 │   ├── sessions not shared: OK
 │   └── history not shared: OK
 └── personal
     ├── auth exists: OK
     └── shared cache: WARN
```

状态：

```text
OK
WARN
ERROR
```

快捷键：

```text
b            返回
r            重新检查
q            退出
```

---

## TUI 弹窗 / 输入框

需要实现：

- 新建 profile 名称输入框
- 删除确认弹窗
- exec prompt 输入框
- 错误提示弹窗
- 成功提示弹窗
- share-cache 确认弹窗
- unshare-cache 确认弹窗

---

## TUI 外部命令执行要求

从 TUI 执行以下命令时：

```text
login
run
exec
shell
```

必须：

1. 暂停 TUI
2. 退出 raw mode
3. 恢复正常 terminal
4. 执行官方 `codex`
5. 继承 stdin/stdout/stderr
6. 命令结束后显示：

```text
Press Enter to return to CodexHub TUI...
```

7. 用户按 Enter 后恢复 TUI

---

# 安全要求

所有 CLI 和 TUI 都必须遵守：

- 不打印 `auth.json` 内容
- 不复制 `auth.json`
- 不共享 `auth.json`
- 不默认允许 `auth.json` symlink
- 不自动轮换账号
- 不调用 ChatGPT / OpenAI 私有 API 查询额度
- 不实现绕过 OpenAI 限制的逻辑
- 不把 token 写入日志
- 不把 token 写入 crash report
- `doctor` 必须能发现共享 auth
- `doctor` 必须能发现共享 session/history/state

如果用户强制允许 auth symlink，必须提供显式危险参数：

```bash
--allow-auth-symlink
```

默认仍然报错。

---

# 技术栈

优先使用 Rust。

依赖建议：

```toml
[dependencies]
anyhow = "1"
clap = { version = "4", features = ["derive"] }
ratatui = "0.29"
crossterm = "0.28"
serde = { version = "1", features = ["derive"] }
toml = "0.8"
dirs = "5"
walkdir = "2"
fs_extra = "1"
chrono = { version = "0.4", features = ["serde"] }
humansize = "2"
```

如果版本不兼容，请选择最新稳定版本并保证项目可编译。

---

# 代码结构建议

```text
src/
├── main.rs
├── cli.rs
├── config.rs
├── profile.rs
├── doctor.rs
├── process.rs
├── size.rs
├── shared.rs
├── shell.rs
└── tui/
    ├── mod.rs
    ├── app.rs
    ├── ui.rs
    ├── events.rs
    ├── screens.rs
    └── widgets.rs
```

---

# 模块职责

## `cli.rs`

- clap CLI 定义
- 子命令路由

## `config.rs`

- 读取 / 写入 `~/.codexhub/config.toml`
- 默认路径管理
- shared cache 配置

## `profile.rs`

- 创建 profile
- 删除 profile
- list profiles
- 计算 profile metadata
- 检查 auth 状态

## `doctor.rs`

- Codex binary 检查
- profile 安全检查
- symlink 检查
- inode 冲突检查
- 共享敏感文件检查

## `process.rs`

- 执行官方 `codex`
- 设置 `CODEX_HOME`
- 继承 stdin/stdout/stderr
- TUI 暂停 / 恢复时调用

## `shared.rs`

- share-cache
- unshare-cache
- 备份目录
- 创建 symlink
- 校验共享白名单

## `size.rs`

- 目录大小统计
- human-readable size

## `shell.rs`

- 启动 profile shell
- 设置 prompt
- 设置 `CODEX_HOME`

## `tui/`

- TUI 状态管理
- 页面渲染
- 键盘事件
- 弹窗
- 输入框
- 外部命令暂停/恢复

---

# README 要求

请生成完整 README，包含：

## 安装

```bash
cargo install --path .
```

或：

```bash
cargo build --release
```

## 快速开始

```bash
codexhub init
codexhub create personal
codexhub login personal
codexhub run personal
```

## 多账号示例

```bash
codexhub create work
codexhub login work

codexhub create personal
codexhub login personal
```

## 并发运行示例

Terminal 1:

```bash
codexhub run work
```

Terminal 2:

```bash
codexhub run personal
```

## Exec 示例

```bash
codexhub exec work -- "检查这个项目"
```

## TUI 示例

```bash
codexhub tui
```

或者：

```bash
codexhub
```

## 共享缓存示例

```bash
codexhub share-cache work
codexhub share-cache personal
```

## 必须解释

README 必须解释：

- 什么是 `CODEX_HOME`
- 为什么每个账号需要独立 `CODEX_HOME`
- 为什么不能复制 `auth.json`
- 为什么不能共享 `sessions/`
- 为什么不能共享 `history.jsonl`
- 为什么不做账号 rotation
- 为什么不调用私有 API 查额度
- 和 `codex-auth` / `codex-multi-auth` / auth.json switcher 的区别

## 截图占位

README 中加入：

```text
[Profile List Screenshot]
[Profile Detail Screenshot]
[Doctor Screenshot]
```

---

# 最终交付要求

请先输出完整架构设计，然后直接实现完整 Rust 项目代码。

必须包含：

```text
Cargo.toml
README.md
src/main.rs
src/cli.rs
src/config.rs
src/profile.rs
src/doctor.rs
src/process.rs
src/size.rs
src/shared.rs
src/shell.rs
src/tui/mod.rs
src/tui/app.rs
src/tui/ui.rs
src/tui/events.rs
src/tui/screens.rs
src/tui/widgets.rs
```

代码要求：

- 能在 macOS 编译运行
- 能在 Linux 编译运行
- 所有路径支持 `~` 展开
- 所有外部命令用 `std::process::Command`
- `login` / `run` / `exec` 必须继承 stdin/stdout/stderr
- 错误信息清晰
- 不要只给伪代码
- 不要省略核心实现
- 不要实现 auth.json 切换器
- 不要实现账号 rotation
- 不要调用私有 API
- `codexhub` 默认进入 TUI
- `codexhub tui` 显式进入 TUI

---

# 最重要的原则

```text
CodexHub 管理的是物理隔离的 CODEX_HOME profile。
它不是 auth.json switcher。
它不复制 auth.json。
它不共享 auth.json。
它不覆盖 ~/.codex/auth.json。
它只是用正确的 CODEX_HOME 启动官方 Codex CLI。
```
