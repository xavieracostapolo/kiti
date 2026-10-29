# Changelog

All notable changes to Kiti are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow [Semantic Versioning](https://semver.org/).

Every pull request with a change people can see adds a line under **Unreleased**. When a version is tagged, that section becomes the version's section and its text opens the GitHub release notes ([docs/release.md](docs/release.md)).

## [Unreleased]

### Added

- Three map layouts, dynamic (centered rows), horizontal and vertical, picked from the top bar or with `?layout=` in the demo.
- `+`, `−` and fit buttons in the corner of the map.
- Working agents are animated, and each office gets its own decor.
- `CHANGELOG.md`, the history of every release.
- The agent's side panel also shows the project's git status, and for Claude the model, context used and session cost.
- An empty map shows a «Sin agentes en ejecución» notice, with buttons to launch Claude Code, Codex CLI or Antigravity CLI when Kiti can open terminals.
- Agents can message each other through Kiti, whatever their CLI: `mise run mcp:install` adds the `kiti_agents`, `kiti_send` and `kiti_inbox` tools to Claude Code, Antigravity CLI and Codex CLI. Kiti types each message into the recipient's terminal when it is free. A ✉ panel lists the threads, and the map shows aliases and envelopes between offices. Delivery is automatic by default, a manual mode asks for approval, and limits on messages per hour and per thread stop loops.
- Project and session memory (ADR 0018): Kiti automatically tracks each session's initial prompt, modified files and task checklist via hooks, and exposes `kiti_remember` and two-step `kiti_recall` (lightweight pulse/title index + on-demand SQLite `FTS5` `bm25` search) in `kiti-mcp` so agents survive context compaction and share project decisions, gotchas and handoffs without bloating the context window.

### Changed

- The layout buttons in the top bar are icons only (⊞ ⊟ ☷), with a tooltip, so the bar fits on narrower windows.
- Quota updates in real time: Antigravity's is read again as soon as one of its agents finishes a turn (at most every 30 s, and every 5 min without turns), and a window whose reset time has passed shows 100 % without waiting for a new reading.

### Fixed

- The office board's box fits its text instead of a fixed width.
- The office board read the wrong repository when Kiti ran inside a git hook that exports `GIT_DIR`.

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

[Unreleased]: https://github.com/xavieracostapolo/kiti/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/xavieracostapolo/kiti/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/xavieracostapolo/kiti/releases/tag/v0.1.0
