# Security policy

## Supported versions

Kiti has no releases yet. Security fixes go to the `main` branch.

## Reporting a vulnerability

Do not open a public issue. Report it privately through [GitHub security advisories](https://github.com/xavieracostapolo/kiti/security/advisories/new).

Include the affected component, the steps to reproduce and the impact. You will get an answer within 7 days.

## Scope

These parts of Kiti handle untrusted input or have privileges, so reports about them are the most useful:

- **Hook server** (`crates/core`, HTTP on `127.0.0.1`): token check, `Host` and `Origin` checks, body limits. It must never return a decision to an agent.
- **`kiti-hook` forwarder** (`crates/forwarder`): runs inside every Codex and Gemini hook. It must fail open and never block the CLI.
- **Closing an office**: sends signals to processes. It must verify `(pid, start_time, uid, cmd)` before each signal.
- **Settings installers**: edit `~/.claude/settings.json`, `~/.codex/hooks.json` and `~/.gemini/settings.json`. They must keep the user's own hooks.
- **Tauri capabilities and CSP** (`src-tauri`): the UI must not reach commands or origins it does not need.
- **Privacy**: prompts, code and tool output must never reach logs or files.
