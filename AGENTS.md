# AGENTS.md: Kiti

Guía para cualquier agente (Claude Code, Codex, Gemini u otros) que trabaje en este repositorio. Léela completa antes de empezar. Actualízala al cerrar cada hito.

## Qué es Kiti

App de escritorio con estética de videojuego 2D cenital (pixel art, estilo Gather.town o Stardew Valley). Muestra una **oficina virtual donde cada sala es un agente de código en ejecución** en la máquina: Claude Code, Codex CLI o Gemini CLI. El objetivo es tener todos los agentes centralizados en una sola vista.

Funciones del MVP:
- Ver solo los agentes que están en ejecución.
- Cámara con zoom a la oficina de un agente, vista general de todos y pan.
- Semáforo de estado por oficina.
- Cerrar una oficina, lo que termina el proceso del agente pidiendo confirmación antes.

Más adelante se añadirán paneles de detalle, logs, terminal embebida y lanzamiento de agentes.

## Estado actual

- **Fase:** Hito 1, "Oficina visual simulada", **cerrado** (2026-10-24) en la rama `hito-1-oficina-visual`: el usuario aprobó el estilo y las baselines `webkit-darwin` están fijadas en `ui/e2e/__screenshots__/` (ver [Roadmap](#roadmap) y el [plan del ADR 0006](docs/decisiones/0006-arquitectura-consolidada/README.md#plan-del-hito-1-oficina-visual-simulada)).
- **Hecho:** `kiti-protocol` con tipos TS generados, simulador TS con conformidad compartida, dominio (layout, cámara, LOD), store, mundo Pixi con render bajo demanda, paneles React, telemetría, E2E y benchmark.
- **Pendientes que deja el Hito 1** (desviaciones del plan, a resolver en iteraciones siguientes):
  - La oficina se dibuja de forma procedural (`world/officeTemplate.ts`); los tilesheets de Kenney, el `.tmj` de Tiled y `domain/tiles` quedan pendientes.
  - Las capturas usan `steps=357` en lugar de `steps=120`: con `seed=42` es el primer paso en el que conviven los cinco colores.
  - `ui.react_commits_total` sube cuando cambian los contadores de la barra superior, no con cada diff.
  - El p95 del delta de rAF en WebKit mide 17 ms (60 fps reales: 601 frames en 10 s) porque WebKit redondea los timestamps a ms enteros; el test tolera 1 ms.
  - Faltan `lefthook`, commitlint, Renovate, `deny.toml` y el workflow de CI.
- **Siguiente paso:** Hito 2, shell Tauri y core con simulador. Cualquier cambio visual se itera con `mise run ui:shots` y, con el visto bueno del usuario, se refijan las baselines con `pnpm -C ui exec playwright test --project=webkit-visual --update-snapshots`.

## Índice de ADRs

| ADR | Una línea |
|---|---|
| [0001](docs/decisiones/0001-stack/README.md) | Stack: Tauri v2, Rust y TS con PixiJS v8; macOS ahora y Windows después (su mención a `pixi-viewport` la sustituye el 0003) |
| [0002](docs/decisiones/0002-backend-rust-ddd/README.md) | Backend DDD: un crate `kiti-core` con *features*, reducer puro del semáforo, ports sin `async-trait`, fakes y `SimFleet` |
| [0003](docs/decisiones/0003-frontend/README.md) | Frontend: Pixi imperativo, zustand vanilla, `Transport` (Sim y Tauri), cámara propia, TS 7 + Biome, Vitest y Playwright |
| [0004](docs/decisiones/0004-integracion-seguridad-observabilidad/README.md) | IPC por Channel y `ts-rs` en `crates/protocol`, mínimo privilegio, HookServer seguro, `tracing`, `mise` y CI |
| [0005](docs/decisiones/0005-integracion-clis-agentes/README.md) | Contrato con los CLIs: hooks como fuente del semáforo, 8 eventos normalizados, detección por `exe`/`argv` y fixtures |
| [0006](docs/decisiones/0006-arquitectura-consolidada/README.md) | Arquitectura consolidada: decisiones de los conflictos, dependencias aprobadas y plan del Hito 1 |

## Decisiones tomadas (no repetir)

El stack se decidió con un debate de agentes y un juez independiente ([ADR 0001](docs/decisiones/0001-stack/README.md)). La arquitectura se consolidó en el [ADR 0006](docs/decisiones/0006-arquitectura-consolidada/README.md). **No reabras estas decisiones** salvo que se cumpla alguna de las condiciones de reapertura de su ADR.

| Tema | Decisión |
|---|---|
| Plataforma | macOS ahora, Windows después. Linux no está previsto |
| Shell de la app | Tauri v2 |
| Backend | Rust (`tokio`), DDD en un crate con *features* |
| UI del mundo 2D | TypeScript + PixiJS v8 imperativo + **cámara propia** (sin `pixi-viewport`) |
| UI de paneles | React, solo para paneles y overlays, nunca en el loop de render |
| Build y tooling de la UI | Vite, TypeScript 7, Biome, pnpm |
| Tipos compartidos | `ts-rs` en `crates/protocol`, generados con `mise run gen-types` y versionados |
| IPC | `tauri::ipc::Channel` para el stream y commands para las acciones. Sin `emit` |
| Observabilidad | `tracing` + métricas propias + panel de debug. Sin OpenTelemetry en el MVP |
| Tests | TDD: Vitest, fast-check y Playwright en la UI; nextest, proptest e insta en Rust |
| Prioridad número uno | Rendimiento con 20 o más agentes simultáneos |

## Arquitectura

```
kiti/
  fixtures/sim/       # perfiles y conformidad compartidos por los dos simuladores
  crates/protocol/    # kiti-protocol: DTOs + PROTOCOL_VERSION + ts-rs (solo serde y ts-rs)
  crates/core/        # kiti-core, Rust puro y SIN Tauri: domain / application / ports / adapters / sim
  crates/forwarder/   # kiti-hook: reenvía los hooks de Codex y Gemini; no depende de nada del repo
  src-tauri/          # capa fina: commands, Channel hacia la UI, capabilities y tracing
  ui/                 # Vite + TS + PixiJS (+ React para paneles); funciona sin Tauri en modo sim
  docs/decisiones/    # ADRs numerados
```

El árbol completo está en el [ADR 0006](docs/decisiones/0006-arquitectura-consolidada/README.md#estructura-del-monorepo).

Reglas de arquitectura:
1. **`crates/core` no depende de Tauri**, y su `domain` no depende de `kiti-protocol` ni hace I/O. CI compila `kiti-core` con `--no-default-features` para comprobarlo.
2. **La UI habla con el backend solo a través de la interfaz `Transport`** (`SimTransport` o `TauriTransport`).
3. **Los tipos del protocolo viven en `crates/protocol`** y se generan a TS. Nunca se duplican a mano.
4. **El semáforo lo calcula el core.** La UI no infiere estados; la única excepción es el simulador TS, que sustituye al core y se valida contra `fixtures/sim/conformance/`.
5. **Los slots y el layout de las oficinas son de la UI.**

### Flujo de datos

```
[claude]  --hook http------------------> [core: HookServer (axum, 127.0.0.1, /v1/hooks)]
[codex]   --hook command--> [kiti-hook] --> [core: HookServer]
[gemini]  --hook command--> [kiti-hook] --> [core: HookServer]
[procesos] --sysinfo cada 2 s----------> [core: ProcessScanner]
[~/.claude/sessions/*.json] --notify---> [core: ClaudeSessionsWatcher]
[*.jsonl] --notify (FSEvents)----------> [core: TranscriptTailer]
                                            |
                              [core: AgentRegistry + reducer + Correlator]
                                            |  ServerMsg::Diff coalescido, máx. ~10/s
                              [src-tauri: tauri::ipc::Channel<ServerMsg>]
                                            |
                              [ui: store -> OfficeWorld (Pixi) + paneles (React)]
```

### Modelo de datos (protocolo v1)

```ts
// Generado por ts-rs desde crates/protocol (ADR 0004)
type AgentState = {
  id: string;                 // "{provider}:{pid}:{start_ms}", opaco
  provider: "claude" | "codex" | "gemini";
  pid: number;
  cwd: string;
  startedAt: number;
  status: { kind: "working" | "waiting" | "idle" | "error" | "dead"; reason: StatusReason; since: number };
  lastTool?: string;
  lastEventAt: number;
};
type ServerMsg = { type: "Snapshot"; seq; agents } | { type: "Diff"; seq; ingestMs; upserts; removed } | { type: "Metrics"; … };
```

### Semáforo

La tabla completa de transiciones está en el [ADR 0002 §3](docs/decisiones/0002-backend-rust-ddd/README.md).

| Color | Estado | Señal |
|---|---|---|
| Verde | `working` | `PromptSubmitted` o `ToolStarted`; en Claude también `status: busy` en `sessions/<pid>.json` |
| Amarillo | `waiting` | `AwaitingInput`: `PermissionRequest` (Claude y Codex) o `Notification` de permiso (Gemini; en Claude solo como respaldo) |
| Rojo | `error` / `dead` | `StopFailure`, o el proceso murió sin `SessionEnded` ni kill. **Un fallo de herramienta no es rojo** |
| Gris | `idle` | `Stop`, `idle_prompt`, `status: idle`, o 30 s sin eventos sin herramientas abiertas |

## Detección de agentes por CLI

Verificado en el [ADR 0005](docs/decisiones/0005-integracion-clis-agentes/README.md) (Claude 2.1.310, Codex 0.164.0 y Gemini 0.65.0).

| CLI | Proceso | Hooks | Sesiones y transcripts |
|---|---|---|---|
| Claude Code | Binario nativo (también en npm), en `~/.local/share/claude/versions/<semver>`. Excluir `Claude.app` | Tipo `http` nativo. Kiti se suscribe a `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `PostToolUseFailure`, `PermissionRequest`, `PermissionDenied`, `Notification`, `Stop`, `StopFailure` y `SessionEnd`. `Stop` **no** llega si el usuario interrumpe con Esc | `~/.claude/sessions/<pid>.json` (pid ↔ sesión, `busy`/`idle`) y `~/.claude/projects/<cwd>/<session>.jsonl` |
| Codex CLI | Binario Rust. Con npm hay **2 procesos** (`node` lanzador y el binario hijo, que es el agente) | **No hay `http`**: `command` vía forwarder en `~/.codex/hooks.json`. El usuario debe **confiar** en el hook en `/hooks`. No hay `Notification`; sí `PermissionRequest` e `Interrupt` | `~/.codex/sessions/AAAA/MM/DD/rollout-*.jsonl` (no es interfaz estable; migra a SQLite) |
| Gemini CLI | **`node`** en todas las instalaciones, con **2 procesos** por sesión (el agente es el hijo relanzado). Excluir `Gemini.app` | Solo `command` y síncronos, vía forwarder, que debe imprimir `{}`. Eventos `BeforeAgent`, `BeforeTool`, `AfterTool`, `AfterAgent`, `Notification`, `SessionStart` y `SessionEnd` | `~/.gemini/tmp/<slug>/chats/session-*.jsonl` |

- **Detecta por `exe` y `argv`, nunca por el nombre del proceso.** Hay un único `classify` en el core, que distingue el agente del lanzador.
- **Correlación PID ↔ sesión:** en Claude, `sessions/<pid>.json`; en Codex y Gemini, el forwarder envía `X-Kiti-Ppid` y el core sube por los ancestros; como respaldo, `cwd` y la hora de inicio con ±5 s.
- El forwarder tiene que **fallar en abierto**: plazo total de 150 ms (1 s en la config del CLI), siempre `exit 0`, y si Kiti está cerrado no pasa nada. Codex y Gemini ejecutan los hooks de forma síncrona.
- Kiti **nunca** devuelve decisiones a un hook: responde `204` vacío.
- **Cerrar un agente:** se verifica `(pid, start_time, uid, cmd)` antes de cada señal; en macOS, SIGTERM y SIGKILL tras 5 s. En Windows, `taskkill /T`. Siempre con confirmación (nativa en Tauri, `AlertDialog` en el simulador).

## Presupuesto de rendimiento

Objetivo: con 20 agentes activos, la app debe ir "como si nada".

- **Reposo:** CPU por debajo del 1 % y RAM de 100–150 MB (estimación; se mide en el Hito 2).
- **Backend:**
  - Los JSONL se leen de forma incremental desde el último offset. Nunca se releen completos.
  - `sysinfo` refresca solo cmd, cwd, exe y usuario, con `OnlyIfNotSet`.
  - Los cambios se agregan y se envían a la UI como diffs, a unos 10 Hz como máximo. Nunca un `emit` por cada evento.
- **UI:**
  - Render bajo demanda: el ticker se detiene cuando nada se anima. **0 frames en reposo.**
  - Tilesheets en rejilla y la plantilla de oficina rasterizada una sola vez.
  - Culling de las oficinas que quedan fuera de cámara.
  - Nivel de detalle: alejado, solo salas y semáforos; cerca, todo el detalle.
  - Con la ventana oculta o minimizada se pausa el render; el core sigue y el estado se ve en la barra de menú.
- **Simulación:** el simulador TS (Hito 1) y `SimFleet` en Rust (Hito 2) generan al menos 20 agentes, escalables a 50, y comparten `fixtures/sim/`.

## Roadmap

Alineado con el [ADR 0006](docs/decisiones/0006-arquitectura-consolidada/README.md).

- [x] **1. Oficina visual simulada:** `crates/protocol` + `ui/` en el navegador con `SimTransport`. Grilla de 20 oficinas, cámara (zoom, pan, doble clic para enfocar), LOD, culling, semáforos, diálogo de cierre, telemetría y panel de debug. Capturas con Playwright en 3 niveles de zoom.
- [ ] **2. Shell Tauri y core con simulador:** `src-tauri` (Channel, capabilities, CSP, `tracing`), dominio y reducer de `kiti-core`, `SimFleet` y `TauriTransport`. Medir RAM y CPU.
- [ ] **3. Procesos reales:** `sysinfo`, `classify` con roles y cerrar oficina de verdad, con verificación.
- [ ] **4. Hooks de Claude Code:** HookServer, token, instalador de hooks, `sessions/<pid>.json` y experimento E1.
- [ ] **5. Forwarder:** hooks para Codex y Gemini.
- [ ] **6. Tailer de transcripts** y panel lateral con la última herramienta, el cwd y el último evento.
- [ ] **7. Barra de menú y bandeja**, y pausa del render con la ventana oculta.
- [ ] **Después:** terminal embebida (`@xterm/xterm`), soporte para Windows y lanzar agentes desde Kiti.

## Pendientes de verificar

- Medir la RAM y la CPU reales en Tauri frente al presupuesto (Hito 2).
- E1: si Claude muestra avisos en el TUI cuando el hook `http` no conecta (Hito 4, requiere permiso).
- El `argv` real de Gemini con npx y Homebrew, y su ruta de chats en la 0.65.0.
- La ruta del cask de Claude y los subcomandos de Codex que no son agentes.
- El evento de Claude al denegar un permiso, y los errores de agente en Codex y Gemini.
- Si Gemini pide confirmación para los hooks de usuario y de qué depende el hash de confianza de Codex.
- Que el *pinch* del trackpad funcione en WKWebView con la cámara propia (prueba manual en el Hito 1).

## Reglas para agentes

1. **TDD obligatorio.** Primero el test en rojo, después el mínimo código que lo pone en verde y por último el refactor. No se aceptan cambios de lógica sin su test. El orden de los primeros tests del Hito 1 está en el ADR 0006.
2. **Ficha de observabilidad por hito**, antes de escribir código: preguntas, señales con nombre, umbrales y cómo se verifican. El hito no se cierra hasta que las señales se vean en el panel de debug o en `window.__kiti.metrics()`.
3. **No ejecutes CLIs de agentes con costo** (`claude`, `codex`, `gemini`, también en `-p` o `exec`) ni nada que gaste dinero o cuota del usuario **sin su permiso explícito**. Eso incluye capturar fixtures y el experimento E1. Para desarrollar, usa los simuladores y los fixtures existentes.
4. **No reinventes la rueda:** usa las dependencias aprobadas del ADR 0006. Cualquier dependencia nueva se justifica en el PR (mantenimiento en los últimos 12 meses, licencia y alternativa).
5. **Itera lo visual con capturas:** levanta la UI en modo sim, captura con Playwright, enseña las capturas al usuario y fija las baselines solo con su visto bueno.
6. Nunca registres ni guardes en fixtures prompts, código ni salidas de herramientas: pasa todo por el anonimizador.

## Cómo trabajar en este repo

- Las nuevas decisiones de arquitectura van en `docs/decisiones/NNNN-titulo/README.md`, con el formato del ADR 0001.
- Al cerrar un hito: márcalo en el roadmap, actualiza "Estado actual" y los comandos.
- **Requisitos:** `mise` y `rustup` (Homebrew: `brew install mise rustup`; `rustup` es *keg-only*, así que añade `/opt/homebrew/opt/rustup/bin` al `PATH`). `rust-toolchain.toml` y `mise.toml` fijan las versiones.
- **Comandos** (definidos en `mise.toml`):

  ```bash
  mise install && mise exec -- pnpm install       # toolchain y dependencias
  mise exec -- pnpm -C ui exec playwright install webkit chromium
  mise run gen-types     # regenera ui/src/protocol/generated desde crates/protocol
  mise run ui:dev        # UI en modo sim: http://127.0.0.1:5173/?sim=20&seed=42
  mise run ui:shots      # con ui:dev levantado: ui/.shots/sim20-{far,mid,near,focus3}.png
  mise run ui:test       # Vitest
  mise run ui:e2e        # Playwright funcional (Chromium)
  mise run ui:visual     # regresión visual (solo macOS, WebKit)
  mise run ui:perf       # 50 agentes, WebKit headed, solo local
  mise run check         # fmt, clippy, tests Rust, Biome, tsc, Vitest con cobertura y E2E
  ```

- **Parámetros de URL del modo sim:** `sim`, `seed`, `steps`, `freeze=1`, `anim=0`, `zoom=far|mid|near`, `focus=<id|índice>`. En la consola, `window.__kiti` expone `step`, `focus`, `setZoom`, `metrics`, `counts`, `frames` y `bench`. El panel de debug se abre con `Cmd+Shift+D`.
- Los recursos gráficos del MVP son de Kenney.nl (CC0). Los packs de oficina de LimeZu en itch.io quedan pendientes de revisar su licencia.
