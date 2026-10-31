# Changelog

All notable changes to Kiti are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow [Semantic Versioning](https://semver.org/).

Every pull request with a change people can see adds a line under **Unreleased**. When a version is tagged, that section becomes the version's section and its text opens the GitHub release notes ([docs/release.md](docs/release.md)).

## [Unreleased]

## [0.3.0] - 2026-10-31

### Added

- **Ephemeral agents.** Agents can start other agents through Kiti with `kiti_spawn`: each one gets its own git worktree and branch, its role sheet and task. When it calls `kiti_done`, Kiti tells the orchestrator, closes its session, archives its conversation and removes the worktree if it is safe.
- **Task board.** A ⚙ panel in the top bar lists ephemeral agents' tasks, grouped by the agent that launched them, with role, branch, model, state and pull request. The map draws a line with an arrow from each orchestrator to its ephemeral agents, and their offices show who launched them and with which role.
- **Messages between agents.** Agents can message each other through Kiti, whatever their CLI: `mise run mcp:install` adds the `kiti_agents`, `kiti_send` and `kiti_inbox` tools to Claude Code, Antigravity CLI and Codex CLI. Kiti types each message into the recipient's terminal when it is free. A ✉ panel lists the threads, and the map shows aliases and envelopes between offices. Delivery is automatic by default, a manual mode asks for approval, and limits on messages per hour and per thread stop loops. `kiti_agents` shows each agent's tmux session and PID.
- **Project and session memory (ADR 0018).** Kiti tracks each session's initial prompt, modified files and task checklist through hooks, and `kiti-mcp` adds `kiti_remember` and a two-step `kiti_recall` (a light index, then a search that ignores accents and matches singular and plural forms). Memories are stored in SQLite and shared by all git worktrees of a repository, so agents keep their bearings after context compaction.
- Three map layouts, dynamic (centered rows), horizontal and vertical, picked from the top bar or with `?layout=` in the demo.
- `+`, `−` and fit buttons in the corner of the map.
- Working agents are animated, and each office gets its own decor.
- The agent's side panel also shows the project's git status, and for Claude the model, context used and session cost.
- An empty map shows a «Sin agentes en ejecución» notice, with buttons to launch Claude Code, Codex CLI or Antigravity CLI when Kiti can open terminals.
- `CHANGELOG.md`, the history of every release.

### Changed

- Quota updates in real time: Antigravity's is read again as soon as one of its agents finishes a turn (at most every 30 s, and every 5 min without turns), and a window whose reset time has passed shows 100 % without waiting for a new reading.
- Offices have a 2.5D diorama look, with depth and shadows; the terminal panel can be resized; and the status counters in the top bar shrink to just the number when they are zero.
- A message's envelope stays on the map for 3 seconds after delivery, so delivery is visible even when it is instant.
- The layout buttons in the top bar are icons only (⊞ ⊟ ☷), with a tooltip, so the bar fits on narrower windows.
- `mise run app:bundle` also reinstalls `kiti-mcp` and `kiti-hook` in `~/.kiti/bin`, so a rebuilt app does not leave agents with stale binaries.

### Fixed

- The messages and tasks panels no longer overlap: opening one closes the other.
- Messages waiting for an agent, open threads and agent aliases survive a Kiti restart, and an agent relaunched in the same Kiti tmux session keeps its alias.
- The office board's box fits its text instead of a fixed width.
- The office board read the wrong repository when Kiti ran inside a git hook that exports `GIT_DIR`.
- A tmux session started outside Kiti (`tmux -L kiti new-session`) gets its terminal in the office without reopening the window.
- Kiti lists its tmux sessions correctly with tmux 3.8, and an embedded terminal resizes to its panel after attaching.
- The debug panel counts memories stored and recalled.

## [0.2.0] - 2026-10-27

### Added

- **Usage quota.** The top bar shows how much of your Claude plan (5-hour and weekly windows) and of your Antigravity quota is left, and Kiti notifies you once when a weekly limit drops to 20 % or less. For Claude, run `mise run statusline:install` from a clone; `statusline:uninstall` restores your status line. For Antigravity, Kiti reads `agy -p /usage` every 5 minutes, which uses no tokens.
- **Office board.** At the closest zoom, each office shows the model, context used and session cost (Claude), the project's git branch, changes and commits to push or pull, and a timeline of the last hour.
- Stable download link: `releases/latest/download/Kiti_universal.dmg` always points to the newest version.

## [0.1.0] - 2026-10-26

First public release: a pixel-art virtual office for macOS where every room is a running coding agent. Universal build (Apple Silicon and Intel) with an ad-hoc signature.

### Added

- Detects Claude Code, Codex CLI, Antigravity CLI and Gemini CLI, with a traffic light per agent driven by their hooks.
- Offices grouped by project, readable from far away, with status filters and `/` search.
- Embedded terminal on persistent `tmux` sessions, resumable Claude and Antigravity conversations, and «Ir a su terminal» for agents opened in Ghostty or Terminal.app.
- Menu bar counters and macOS notifications that open the office on click.
- Pixel art from Kenney, 0 frames at rest, built for 20 or more agents.

[Unreleased]: https://github.com/xavieracostapolo/kiti/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/xavieracostapolo/kiti/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/xavieracostapolo/kiti/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/xavieracostapolo/kiti/releases/tag/v0.1.0
