# Contributing to Kiti

Thanks for your interest in Kiti. This guide explains how to set up the project and what a pull request needs.

## Before you start

- Read [AGENTS.md](AGENTS.md). It is the full guide to the codebase: architecture rules, the status traffic light, the performance budget and every `mise` command. It is in Spanish, like the ADRs.
- For a large change, open an issue first. Architecture decisions live in [`docs/decisiones`](docs/decisiones) and are not reopened unless one of the reopening conditions in the ADR applies.

## Setup

You need macOS, [`mise`](https://mise.jdx.dev) and `rustup`. `tmux` is optional.

```bash
brew install mise rustup tmux
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"   # rustup is keg-only
mise install && mise exec -- pnpm install
mise exec -- pnpm -C ui exec playwright install webkit chromium
```

Git hooks are installed by `pnpm install` (lefthook). Pre-commit runs `rustfmt` and Biome. Pre-push runs clippy, the type check and the unit tests.

## Development loop

```bash
mise run ui:dev        # UI with simulated agents at http://127.0.0.1:5173/?sim=20&seed=42
mise run app:sim       # native app with simulated agents
mise run check         # everything that must be green before a pull request
```

Use the simulators and the fixtures in `fixtures/sim/`. You do not need a real agent to work on Kiti.

## Rules

1. **Test-driven development.** Write the failing test first, then the minimum code that makes it pass, then refactor. Logic changes without a test are not accepted.
2. **Respect the architecture.** `crates/core` never depends on Tauri. The UI talks to the backend only through the `Transport` interface. Protocol types live in `crates/protocol` and are generated with `mise run gen-types`, never copied by hand. The core computes the status; the UI does not infer it.
3. **Performance is the first priority.** Kiti must stay smooth with 20 or more agents. Keep the render on demand (0 frames at rest) and never send one message per event to the UI.
4. **Privacy.** Never log or store prompts, code or tool output, including in fixtures.
5. **Do not run paid agent CLIs in tests or scripts.** `claude`, `codex` and `gemini` cost money or quota.
6. **New dependencies** need a reason in the pull request: maintenance in the last 12 months, license and the alternative you considered. `cargo deny` checks Rust licenses and advisories.
7. **Visual changes** need screenshots. Run `mise run ui:shots` with `ui:dev` running and attach the images to the pull request.

## Commits and pull requests

- Commit messages follow [Conventional Commits](https://www.conventionalcommits.org) (`feat:`, `fix:`, `docs:`, `ci:`…). commitlint checks them.
- Keep one topic per pull request and fill in the template.
- CI must be green: lint, Rust (clippy and tests on macOS), UI unit and E2E tests.

## License

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).
