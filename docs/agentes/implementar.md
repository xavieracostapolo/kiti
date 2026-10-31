# Ficha de rol: implementar

Primero lee [comun.md](comun.md).

## Qué haces

Escribes el código que pide tu encargo, con sus tests, en uno o varios commits pequeños.

## Cómo

1. **TDD:** primero el test en rojo; después, el mínimo código que lo pone en verde; por último, el refactor. No hay cambio de lógica sin su test (regla 1 de AGENTS.md).
2. **Respeta la arquitectura de AGENTS.md:**
   - `crates/core` no depende de Tauri, y su `domain` no hace I/O;
   - el I/O va detrás de un port en `ports.rs`, con un fake para los tests;
   - la UI solo habla con el backend por `Transport`;
   - los tipos del protocolo se generan con `mise run gen-types`.
3. **Rendimiento (prioridad número uno):**
   - 0 frames en reposo;
   - nada de sondeos continuos;
   - nada de I/O bloqueante en el actor del runtime;
   - diffs coalescidos a ~10 Hz como máximo.
4. **Observabilidad:** si el encargo trae ficha, emite las señales con el nombre que dice (`info!(event = "...")`).
5. **Cambio visible para el usuario:** añade una línea en `## [Unreleased]` de CHANGELOG.md (en inglés: Added, Changed, Fixed o Removed).
6. **Capturas para el usuario:** guárdalas en `~/.kiti/shots/<slug>/`, nunca dentro del worktree: Kiti lo borra al cerrar la tarea (y con él lo no commiteado). Pon las rutas absolutas en el `summary` de `kiti_done`.
7. **No toques** los ADR ni AGENTS.md: los actualiza el orquestador o un agente de [documentar](documentar.md).

## Antes de cada commit

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- Si tocas la UI:
  - `mise exec -- pnpm -C ui exec biome check --write src`
  - `mise exec -- pnpm -C ui exec tsc --noEmit -p .`
  - `mise exec -- pnpm -C ui exec vitest run`
  - Si cambia algo del mundo o de los paneles, también `mise run ui:e2e`.
