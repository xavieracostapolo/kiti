# ADR 0006: Arquitectura consolidada y plan del Hito 1

- **Fecha:** 2026-10-24
- **Estado:** Aceptada
- **Decisión:** Se unifican los ADRs 0002 a 0005, que se escribieron en paralelo. El primer hito es la "Oficina visual simulada": solo la UI en el navegador, con el simulador TS y sin backend.

## Componentes

```
 claude ─ hook http ───────────────────┐
 codex  ─ hook command ─► kiti-hook ───┤ POST /v1/hooks/{provider}  (127.0.0.1 + token)
 gemini ─ hook command ─► kiti-hook ───┘
 ~/.claude/sessions/<pid>.json ─ notify ─┐
 transcripts *.jsonl ───────── notify ───┤
 procesos ──────────────── sysinfo 2 s ──┤
                                         ▼
 kiti-core   adapters (ACL) ─► Observation ─► Runtime (actor, sin Mutex)
             domain: AgentRegistry · reducer · Correlator · classify
             application: coalescer 100 ms ─► projection ─► ServerMsg
                                         │  kiti-protocol (DTOs + ts-rs)
 src-tauri   commands (subscribe, kill_agent…) · Channel<ServerMsg> · tracing
                                         │
 ui          Transport (Tauri | Sim) ─► store zustand ─┬► world (Pixi + cámara propia)
                                                       └► panels (React)
```

## Estructura del monorepo

```
kiti/
  AGENTS.md  Cargo.toml  rust-toolchain.toml  deny.toml  .cargo/config.toml
  package.json  pnpm-workspace.yaml  mise.toml  lefthook.yml  biome.json  renovate.json
  .github/workflows/ci.yml
  fixtures/sim/profiles.json          # parámetros de los dos simuladores
  fixtures/sim/conformance/*.json     # señales → estado esperado (Rust y TS)
  crates/protocol/   kiti-protocol: DTOs, PROTOCOL_VERSION, ts-rs        (Hito 1)
  crates/core/       kiti-core: domain/ application/ ports.rs adapters/ sim/ testing/
  crates/forwarder/  kiti-forwarder, binario kiti-hook
  src-tauri/         capa fina: commands, Channel, capabilities, tracing   (Hito 2)
  ui/
    src/{protocol/generated, transport/{sim,tauri}, domain, state, telemetry, world/camera, panels, dev}
    assets/{tiles,characters,icons,maps,fonts}  e2e/  e2e/__screenshots__/  .shots/ (ignorada)
  docs/decisiones/0001…0006
```

## Dependencias aprobadas

Versiones verificadas el 2026-10-24. Cualquier dependencia nueva se justifica en el PR (ADR 0004).

| Nombre | Versión | Capa |
|---|---|---|
| tokio | ≥ 1.54.1 | core, src-tauri |
| axum · tower · tower-http | 0.8.9 · 0.5.3 · 0.7.1 | core (adapter-http) |
| sysinfo | 0.39.6 | core (scanner, kill) |
| notify · notify-debouncer-full | 8.2 · 0.7.0 | core (sesiones, transcripts) |
| serde · serde_json | 1.0.229 · 1.0.154 | todos los crates |
| thiserror · smallvec | 2.0.21 · 1.16 | core, protocol |
| tracing · hdrhistogram | 0.1.44 · 7.6.0 | core (application) |
| getrandom · constant_time_eq | 0.4.3 · 0.6.1 | core (token) |
| rand · rand_chacha | 0.10.3 · 0.10.0 | core (sim) |
| ts-rs | 12.0.1 | protocol |
| ureq (sin features) | 3.5.0 | forwarder |
| tauri · tauri-build | 2.12.5 · 2.7.4 | src-tauri |
| tauri-plugin-log · -dialog · -single-instance | 2.10.1 · 2.8.3 · 2.5.4 | src-tauri |
| tracing-subscriber · tracing-appender · anyhow | 0.3.23 · 0.2.5 · 1.0.104 | src-tauri |
| proptest · insta · criterion · tracing-test | 1.12.0 · 1.49.0 · 0.8.2 · 0.2.6 | tests Rust |
| pixi.js (exacta) | 8.22.0 | ui/world |
| react · react-dom · @vitejs/plugin-react | 19.3.1 · 19.3.1 · 6.1.2 | ui/panels |
| zustand · radix-ui | 5.0.16 · 1.8.1 | ui/state, ui/panels |
| pure-rand | 8.4.2 | ui/transport/sim |
| @tauri-apps/api · @tauri-apps/cli | 2.12.4 | ui/transport/tauri (Hito 2) |
| vite · typescript | 8.3.5 · 7.0.2 | ui (build, tipos) |
| @biomejs/biome | 2.6.2 | ui (lint y formato) |
| vitest · @vitest/coverage-v8 · jsdom | 5.0.5 · 5.0.5 · 30.1.3 | tests ui |
| @testing-library/react · user-event · fast-check | 16.3.3 · 14.6.8 · 4.10.2 | tests ui |
| @playwright/test | 1.64.1 | tests ui, capturas |
| Node · pnpm · Rust (MSRV) | 24.22.1 · 12.15.0 · 1.99.1 (1.95) | tooling (mise, rustup) |
| cargo-nextest · cargo-deny · lefthook · commitlint | 0.9.149 · 0.20.2 · 2.2.2 · 21.2.4 | tooling |

**Descartadas:** `pixi-viewport`, `@assetpack/core`, `async-trait`, `mockall`, `cargo-audit`, `keyring`, OpenTelemetry, ESLint, `typescript-eslint` y Prettier. Biome 2.6.3 se publicó hoy y no cumple la antigüedad mínima de pnpm: se usa 2.6.2 hasta que Renovate la suba.

## Decisiones de los conflictos

1. **Protocolo:** en el crate `crates/protocol`, no en `core::application::protocol`. Compila en segundos y permite generar los tipos en el Hito 1 sin backend. `application::projection` traduce del dominio a los DTOs, y ni el dominio ni el forwarder lo importan.
2. **Baselines visuales:** se generan y comparan solo en macOS con WebKit (`-webkit-darwin`), en el Mac del usuario y en el job `ui-macos`, que es informativo en el Hito 1. En Ubuntu corren Vitest y Playwright funcional con Chromium, sin capturas. El rendimiento se mide solo en local, en modo *headed*.
3. **Cámara propia mínima**, sin `pixi-viewport`. Verificado: el issue #523 sigue abierto y sin comentarios, no hay *push* desde el 2025-02-03 ni ningún fork con actividad o usuarios relevantes, y el error es de API, no de una copia duplicada. Fijar Pixi para siempre sería depender de algo muerto. Lo puro está en `domain/camera.ts` (zoom en torno a un punto, pan, *clamp*, encuadre e interpolación); `CameraInput.ts` traduce Pointer Events, rueda y `GestureEvent` de WebKit.
4. **TypeScript 7.0.2 + Biome.** Es la versión estable (`latest` desde el 2026-07-08). Vite y Vitest no usan `tsc`. `typescript-eslint` exige `<6.1.0` hasta que la API llegue en TS 7.1. Riesgo: el editor necesita la extensión de TS 7.
5. **tokio:** cierto. RUSTSEC-2026-0384 afecta solo a la 1.54.0 (*yanked* en crates.io); se exige `≥ 1.54.1`.
6. **Eventos:** el reducer consume los 8 eventos del ADR 0005 como `Signal::*`, más `SessionFile` de `~/.claude/sessions/<pid>.json`. Esa observación es la primera regla del `Correlator` para Claude y cubre la interrupción con Esc. Amarillo = `PermissionRequest`; `idle_prompt` es gris; un fallo de herramienta no es rojo; `SessionEnded` hace esperada la salida. `Stale`: 30 s, o 10 min con herramientas abiertas.
7. **ts-rs:** los derives están en `crates/protocol`; los tipos se generan con `mise run gen-types` (`cargo test -p kiti-protocol export_bindings`) en `ui/src/protocol/generated/`, fuera del build, y se versionan. CI comprueba con `git diff --exit-code` que no han cambiado.
8. **Versiones:** `mise.toml` fija Node 24.22.1 LTS y pnpm 12.15.0. `rust-toolchain.toml` fija Rust 1.99.1, y `Cargo.toml` declara `rust-version = "1.95"`.
9. **Dos simuladores, justificados.** El simulador TS hace funcionar la UI en el navegador y en CI sin Rust; `SimFleet` sirve para los tests, las benches y la demo en Tauri. Para que no diverjan, comparten `fixtures/sim/profiles.json` (perfiles y medias) y `fixtures/sim/conformance/*.json` (señales y estado esperado), que los dos ejecutan en sus tests. Cada implementación es determinista con su semilla, pero no se exige la misma secuencia entre lenguajes.

**Otros conflictos resueltos:**

- Los slots pasan del core a la UI.
- HookServer: ruta `/v1/hooks`, body de 4 MiB y token obligatorio.
- Forwarder: plazo de 150 ms, bytes crudos, `{}` en stdout solo para Gemini y `getppid` (el core recorre los ancestros).
- Kill en el simulador: desaparece, no pasa a `dead`. Una sola confirmación, gracias a `killConfirmation`.
- CSP: `pixi.js/unsafe-eval`, sin `ScrollArea` y `'unsafe-inline'` solo en `devCsp`.
- `npm` pasa a `pnpm` en todos los comandos.
- AssetPack se sustituye por los tilesheets de Kenney en rejilla y los hashes de Vite.

## Riesgos abiertos

- Los formatos de los CLIs cambian cada semana (Claude, Codex) y `sessions/<pid>.json` no está documentado.
- Experimento E1 (avisos en el TUI de Claude); necesita una sesión real de pago.
- El *pinch* en WKWebView no se puede automatizar con Playwright: hace falta prueba manual.
- Playwright WebKit frente a WKWebView y versiones nuevas de macOS: puede no ser idéntico.
- La RAM y la CPU reales en Tauri siguen sin medirse (Hito 2).

## Plan del Hito 1: "Oficina visual simulada"

**Alcance:** `crates/protocol`, `fixtures/sim/` y `ui/` con `SimTransport`, cámara, LOD, culling, semáforos, diálogo de cierre, telemetría y panel de debug. Sin Tauri ni core.

**Orden TDD** (rojo → verde → refactor):

1. `kiti-protocol`: el JSON de `ServerMsg` (`type`, camelCase) y `export_bindings`.
2. `domain/layout`: `assignSlots` y `slotRect`, más la propiedad de que no se solapan.
3. `domain/camera`: `zoomAt` mantiene fijo el punto, *clamp*, `fitAll`, `focusTarget` e interpolación con tiempo inyectado.
4. `domain/lod` con histéresis y `domain/tiles` (frames desde el `.tmj`).
5. `transport/sim`: conformidad con los fixtures, la misma semilla da el mismo mundo, independencia por agente, `seq` monótono y kill → desaparece.
6. `state`: `applyBatch` conserva referencias, un hueco en `seq` lleva a resync, y `planScene`.
7. `world/RenderScheduler` con rAF falso: agrupa peticiones, se detiene y cuenta frames; `telemetry`: percentiles.
8. `panels`: `CloseOfficeDialog` y contador de renders de los selectores.
9. E2E: humo, reposo, cámara y kill; después `webkit-visual` y `perf`.

**Ficha de observabilidad**

| Pregunta | Señal | Umbral |
|---|---|---|
| ¿Pinta algo en reposo? | `ui.frames_total`, `ui.ticker_running` | 0 frames en 2 s |
| ¿Cuánto tarda un diff en verse? | `ui.diff_to_paint_ms` | p95 < 50 ms |
| ¿Aguanta 50 agentes al hacer pan? | `ui.raf_delta_ms`, `ui.frame_ms` | p95 ≤ 16,7 ms y ≤ 8 ms |
| ¿Funciona el culling? | `ui.offices_visible`, `ui.lod` | < total en `near` |
| ¿React se libra de los diffs? | `ui.react_commits_total` | 0 sin selección |

Además hay logs estructurados por consola (`world.ready`, `transport.connected`, `transport.resync`, `camera.focus`, `kill.requested`). Todo se lee con `window.__kiti.metrics()` y en el panel de debug (`Cmd+Shift+D`). Se verifica con Vitest y Playwright.

**Criterios de aceptación**

1. `mise run ui:shots` genera en `ui/.shots/` cuatro PNG de 1440×900 (`far`, `mid`, `near` y `focus3`) con 0 errores de consola.
2. `far` muestra las 20 oficinas y sus semáforos; `mid` añade los nombres; `near` añade el interior, el avatar y la última herramienta.
3. Con la semilla fijada aparecen los 5 colores (`__kiti.counts()`).
4. Dos capturas de la misma URL son idénticas píxel a píxel en la misma máquina.
5. Con `?sim=20&freeze=1`: 0 frames en 2 s. Con la simulación activa, como mucho un render por `Diff`.
6. Con `?sim=50&seed=1&bench=pan`, se cumplen los umbrales de la ficha (en local, WebKit *headed*).
7. El doble clic enfoca en ≤ 400 ms, el zoom respeta sus límites y el pan no saca el mundo de la vista.
8. Cerrar una oficina: si se confirma, desaparece en el siguiente paso; si se cancela, no se llama a `killAgent`.
9. `mise run check` en verde, con cobertura ≥ 90 % en `domain/`, `state/` y `transport/sim/`.
10. El usuario revisa las capturas. Las baselines se fijan solo con su visto bueno.

**Comandos** (contrato del hito; los define el scaffold):

```bash
mise install && pnpm install --frozen-lockfile
pnpm -C ui exec playwright install webkit chromium
mise run gen-types                       # solo si cambia crates/protocol

pnpm -C ui dev --host 127.0.0.1 --port 5173 --strictPort &   # en segundo plano
curl -s --retry 30 --retry-connrefused --retry-delay 1 -o /dev/null http://127.0.0.1:5173/

for z in far mid near; do                # equivale a: mise run ui:shots
  pnpm -C ui exec playwright screenshot --browser=webkit --viewport-size=1440,900 \
    --wait-for-selector='[data-world-ready="true"]' \
    "http://127.0.0.1:5173/?sim=20&seed=42&steps=120&freeze=1&anim=0&zoom=$z" .shots/sim20-$z.png
done
pnpm -C ui exec playwright screenshot --browser=webkit --viewport-size=1440,900 \
  --wait-for-selector='[data-world-ready="true"]' \
  "http://127.0.0.1:5173/?sim=20&seed=42&steps=120&freeze=1&anim=0&focus=3" .shots/sim20-focus3.png

mise run check                           # biome ci, tsc, vitest, playwright chromium, cargo test -p kiti-protocol
mise run ui:visual                       # solo macOS: --project=webkit-visual
mise run ui:perf                         # 50 agentes, headed
```

## Plan del Hito 2: "Shell Tauri y core con simulador"

**Alcance:** `crates/core` (dominio, reducer, registro, coalescedor, proyección, telemetría y `SimFleet`), `src-tauri` (Channel, commands, capabilities, CSP, `tracing` y diálogo nativo) y `TauriTransport`. Sin procesos reales, HookServer ni forwarder.

**Decisiones del arranque (2026-10-24):**

1. **`SessionEnded` deja de eliminar.** El fixture `salidas.json` y el simulador TS decían `SessionEnded → removed`, en contra del ADR 0002 §3. Con `/clear`, Claude emite `SessionEnd` sin que muera el proceso. Ahora `SessionEnded → idle` (salida esperada) y el `ProcessGone` posterior elimina el agente.
2. **Plugins:** solo `tauri-plugin-dialog` (confirmación nativa). `tauri-plugin-log` y `tauri-plugin-single-instance` pasan al Hito 4, cuando haya HookServer.
3. **Icono:** pixel art propio, pasado por `tauri icon`.

**Orden TDD** (rojo → verde → refactor):

1. Fixture `salidas.json` y simulador TS alineados con el ADR 0002.
2. Esqueleto de `kiti-core` con *features* (`sim`, `testing`); compila con `--no-default-features`.
3. Dominio: value objects, `Signal`, `Observation` y `Agent::apply`, con las filas del ADR 0002 §3, el runner de conformidad y proptest.
4. `StalePolicy` vía `Tick`: 30 s, 10 min con herramientas abiertas y TTL de `Dead` de 60 s.
5. `AgentRegistry` y `RegistryDiff`; `application::projection` hacia `kiti_protocol`.
6. `Coalescer` y `Runtime` (actor) con `tokio::test(start_paused)`: `Snapshot` primero, `seq` monótono, ≤ 1 `Diff` cada 100 ms.
7. `application::telemetry` y `CoreMetrics` a 1 Hz solo con el stream de debug.
8. `sim::SimFleet`: ChaCha8 por agente, `profiles.json`, determinismo y `KITI_SIM=N:SEED[:freeze]`.
9. Cerrar oficina: port `AgentKiller` con `SimKiller`.
10. `src-tauri`: `AppManifest`, capability sin `core:default`, CSP, commands finos, diálogo nativo y `tracing`.
11. `TauriTransport` con `@tauri-apps/api/mocks`; `main.tsx` lo carga con `import()`.
12. Medición de RAM y CPU con 20 y 50 agentes.

**Ficha de observabilidad**

| Pregunta | Señal | Umbral |
|---|---|---|
| ¿Cuánta RAM usa la app con 20 agentes? | `core.rss_bytes` + `footprint` de la app y su WebContent | 100–150 MB en total |
| ¿CPU en reposo? | `core.cpu_percent` con `KITI_SIM=20:42:freeze` | < 1 % |
| ¿Cuánto tarda una observación en salir por el Channel? | `core.ingest_to_send_ms` | p95 < 120 ms |
| ¿Y en pintarse? | `ui.ingest_to_paint_ms` | p95 < 200 ms |
| ¿Se respeta la coalescencia? | `core.ipc_flush_total`, `core.ipc_batch_bytes`, span `ipc.flush` | ≤ 10 Diff/s y 0 Diff sin cambios |
| ¿Core y simulador TS dicen lo mismo? | Suite de conformidad | 100 % en Rust y TS |
| ¿Se pierden diffs? | `ui.resync_total` | 0 en 10 min |
| ¿El panel cuesta algo cerrado? | `core.metrics_msgs_total` | 0 sin `set_debug_stream(true)` |
| ¿El reducer escala? | criterion `registry/*` | < 1 µs por observación y < 50 µs por flush con 50 agentes |

Logs y spans: `app.start`, `ipc.subscribe`, `ipc.flush`, `sim.tick`, `kill.audit` y un resumen de métricas cada 60 s, en JSON con rotación diaria. Se verifica con `tracing-test`, `tokio::time` pausado, Vitest y el panel de debug de la app.

**Criterios de aceptación**

1. `mise run app:dev` abre Kiti con 20 oficinas de `SimFleet` (`KITI_SIM=20:42`), semáforos que cambian, 0 errores de consola y 0 violaciones de CSP.
2. Conformidad al 100 % en Rust y en TS.
3. `kiti-core` compila con `--no-default-features` y `cargo tree -p kiti-core` no contiene `tauri`.
4. Cerrar una oficina pide **un** diálogo nativo: confirmar la quita y cancelar no hace nada.
5. El panel de debug muestra RSS, CPU, eventos/s y el p95 de `ingest_to_send_ms` de la app real.
6. RAM y CPU medidas y registradas. Si se supera el presupuesto, se investiga antes del Hito 3.
7. `mise run check` en verde, con clippy y los tests de `kiti-core` y `src-tauri`.

## Resultados del Hito 2

**Revisión del trabajo de Gemini (2026-10-24).** El core llegó bien diseñado y probado, pero con fallos que la revisión y la ejecución real destaparon:

| Problema | Corrección |
|---|---|
| La app entraba en pánico al arrancar (`tokio::spawn` fuera de un runtime) | `Runtime::new` devuelve el actor; `src-tauri` lo arranca en `setup` sobre el runtime de Tauri |
| El runtime leía `SystemTime` y `SimFleet` usaba el epoch del fixture: todos los agentes en verde pasaban a gris cada 2 s | Toda la hora sale del port `Clock` (`SystemClock`, `ManualClock`); `SimFleet::starting_at` alinea el reloj virtual con el real |
| `handle_kill` esperaba la gracia de 5 s dentro del actor y congelaba los diffs | El kill corre en otra tarea y vuelve con `KillFinished` |
| Un crash de `SimFleet` no emitía `ProcessGone`: el rojo de "caído" nunca aparecía | `step()` devuelve `AgentInput` e incluye `ProcessGone` |
| `AgentId` desconocido se trataba como Gemini | `AgentId: FromStr` validado; un id inválido da `NotFound` |
| `freezePrototype: true` rompía el bundle de Pixi y la UI no arrancaba | Desactivado (Pixi asigna sobre prototipos al cargar) |
| El `EventSystem` de Pixi mantenía un rAF a 60 Hz en reposo (5,4 % de CPU) | `Ticker.system.stop()`; test E2E de 0 llamadas a rAF en reposo |
| Diálogo nativo bloqueante, logs sin JSON ni rotación, panel de debug sin datos del core | `show` asíncrono, `tracing-appender` diario en JSON, `ui_log`, `SelfMonitor` y `set_debug_stream` desde el panel |

**Medición** (`mise run app:measure`, release, MacBook Air, 15 s de calentamiento y 15 s de muestra). La RAM es `footprint`, lo que muestra el Monitor de Actividad. La CPU es el tiempo de CPU consumido en la ventana.

| Escenario | kiti-app | WebContent | GPU | Networking | **Total RAM** | **CPU** |
|---|---|---|---|---|---|---|
| 0 agentes, reposo | 32 MB | 135 MB | 17 MB | 6 MB | **190 MB** | 0 % |
| 20 agentes, reposo (`freeze`) | 32 MB | 142 MB | 17 MB | 6 MB | **197 MB** | **0–0,06 %** |
| 20 agentes, activa | 32 MB | 155 MB | 81 MB | 6 MB | **274 MB** | 4,0 % |
| 50 agentes, activa | 33 MB | 166 MB | 83 MB | 6 MB | **288 MB** | 7,0 % |

Lectura:

- **CPU en reposo:** cumple (< 1 %).
- **RAM:** la estimación de 100–150 MB no era realista para WKWebView con WebGL. La base sin agentes ya son 190 MB y el core en Rust ocupa solo 32 MB. Cada agente cuesta ~0,5–1 MB; el proceso GPU pasa de 17 a ~80 MB mientras hay animación.
- **Presupuesto revisado, aceptado por el usuario (2026-10-24):** ≤ 220 MB en reposo y ≤ 320 MB activa con 50 agentes (medido: 197 y 288 MB).
- **Palancas si hace falta bajar:** destruir el renderer con la ventana oculta (Hito 7, bandeja), canvas a resolución 1 en lugar de 2 (texto menos nítido) y atlas de texturas más pequeños.
- Pendiente de la ficha: `core.ingest_to_send_ms` como histograma propio (hoy se mide el tiempo de publicación) y `ui.resync_total`.

## Hitos siguientes

2. **Shell Tauri y core con simulador:** `src-tauri` con Channel, capabilities, CSP y `tracing`; dominio y reducer de `kiti-core` contra la conformidad; `SimFleet` con `KITI_SIM=20:42`; `TauriTransport`; medir RAM y CPU.
3. **Procesos reales:** `SysinfoScanner`, `classify` con roles y cierre real verificado.
4. **Hooks de Claude:** HookServer, token, instalador portado de Pixel Agents, `sessions/<pid>.json`, E1 (con permiso).
5. **Forwarder** para Codex y Gemini.
6. **Transcripts** y panel lateral.
7. **Barra de menú, bandeja** y pausa del render.

**Después:** terminal embebida, Windows y lanzar agentes.
