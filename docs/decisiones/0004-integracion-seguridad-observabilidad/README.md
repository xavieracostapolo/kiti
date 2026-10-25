# ADR 0004: Integración IPC, seguridad, observabilidad y tooling

- **Fecha:** 2026-10-24
- **Estado:** Aceptada con cambios (ver la revisión al final)
- **Decisión:** Stream de estado por `tauri::ipc::Channel` y comandos para las acciones. Tipos Rust→TS con `ts-rs`, protocolo versionado con un entero y `seq`. Mínimo privilegio: capabilities con `AppManifest::commands`, CSP estricta y sin plugins `shell`/`fs`/`http`. HookServer en `127.0.0.1` con token en un archivo `0600`, validación de `Host`/`Origin` y límite de body. Kill verificado por `(pid, start_time, uid, cmd)` y con confirmación nativa. Observabilidad con `tracing` directo (JSON y rotación diaria), métricas propias y panel de debug. Sin OpenTelemetry en el MVP. Monorepo con cargo y pnpm workspaces, `mise` como task runner, `lefthook`, Renovate y `cargo-deny`.

## Contexto

El ADR 0001 fijó Tauri v2, Rust y TS/PixiJS. Falta decidir cómo se hablan backend y UI, cómo se protege una app que escucha en un puerto local y puede matar procesos, cómo se observa el rendimiento con 20 o más agentes y cómo se organiza el repositorio. Todas las versiones se verificaron en crates.io, npm y GitHub el 2026-10-24.

| Componente | Versión | Estado |
|---|---|---|
| `tauri` / `@tauri-apps/api` | 2.12.5 / 2.12.4 | Estable (v3 en alpha.5) |
| `ts-rs` | 12.0.1 | Estable |
| `tauri-specta` / `specta` | 2.0.0-rc.26 / 2.0.0-rc.27 | **RC** desde 2024 |
| `typeshare` | 1.0.5 | Estable |
| `tracing` / `tracing-subscriber` / `tracing-appender` | 0.1.44 / 0.3.23 / 0.2.5 | Estables |
| `tauri-plugin-log` | 2.10.1 | Estable |
| `opentelemetry` / `tracing-opentelemetry` | 0.33.1 / 0.34.0 | 0.x, cambios frecuentes |
| `axum` / `tower-http` | 0.8.9 / 0.7.1 | Estables |
| `hdrhistogram` | 7.6.0 | Estable |
| `cargo-deny` / `cargo-audit` | 0.20.2 / 0.22.2 | Activos |
| `lefthook` / `mise` / `just` | 2.2.2 / 2026.10.19 / 1.59.0 | Activos |
| Node / pnpm | 24.22.1 LTS (Krypton) / 12.15.0 | Estables |
| Rust | 1.99.1 estable (MSRV 1.95 por `sysinfo`) | Estable |

## 1. IPC Tauri↔UI

### Channel, events o commands

| Mecanismo | Orden | Tipado | Uso en Kiti |
|---|---|---|---|
| `tauri::ipc::Channel<T>` | Garantizado | Genérico `T` | **Stream de diffs (~10 Hz) y métricas** |
| Commands (`invoke`) | Petición/respuesta | Args y retorno | **Acciones: `subscribe`, `kill_agent`, `report_ui_metrics`, `set_debug_stream`** |
| Events (`emit`) | No garantizado con listeners async | JSON sin tipo | **No se usan** |

Según la documentación de Tauri, los eventos "no están diseñados para baja latencia ni alto throughput" y evalúan JS directamente; para streaming remite a los channels. La UI abre el stream con un command que recibe el channel:

```rust
#[tauri::command]
async fn subscribe(on_msg: Channel<ServerMsg>, state: State<'_, Core>) -> Result<Hello, IpcError> {
    state.attach(on_msg).await // envía primero Snapshot y después Diff coalescidos cada 100 ms si hay cambios
}
```

La UI encapsula esto en su interfaz `Transport` (regla 2 de AGENTS.md). `focus_agent` (traer la terminal al frente) queda reservado para después del MVP. El zoom de cámara es solo de la UI.

### Generación de tipos

| Opción | Estado | Pros | Contras |
|---|---|---|---|
| **`ts-rs` 12** | Estable, ~8.9 M descargas recientes | Compatible con serde (`tag`, `rename_all`, `flatten`), sin acoplarse a Tauri | No genera los wrappers de los comandos |
| `tauri-specta` 2 + `specta` 2 | RC 26/27, sin versión estable en más de dos años | Genera los wrappers de commands y events | API cambiante, `specta-typescript` en 0.0.x y acopla `crates/core` a specta |
| `typeshare` | Estable | Multilenguaje | Pensado para FFI. Soporte de genéricos y serde más limitado |
| `schemars` + json-schema-to-ts | Estable | Estándar | Dos pasos y tipos menos idiomáticos |

**Decisión: `ts-rs`**, como ya fijaba AGENTS.md. Los wrappers de los cuatro comandos se escriben a mano en `Transport`, usando los tipos generados. Son pocos y cambian poco.

**Dónde y cuándo se generan.** Los tipos viven en `crates/protocol` (`kiti-protocol`) con `#[derive(Serialize, Deserialize, TS)]` y `#[ts(export)]`. **No forman parte del build de Vite ni de `cargo build`**: se generan a mano con `mise run gen-types` (`cargo test -p kiti-protocol export_bindings`) en `ui/src/protocol/generated/` y se versionan en git. Así la UI arranca sin toolchain de Rust y el Hito 1 no necesita backend. El job `lint` de CI regenera y falla si `git diff --exit-code ui/src/protocol/generated` detecta cambios. En `.cargo/config.toml`:

```toml
[env]
TS_RS_EXPORT_DIR = { value = "ui/src/protocol/generated", relative = true }
TS_RS_LARGE_INT = "number"   # serde_json emite números, no bigint; los timestamps en ms caben en 2^53
```

Se reabre si `tauri-specta` publica la 2.0 estable y los comandos pasan de ~10.

### Versionado del protocolo

- `PROTOCOL_VERSION: u32` en Rust. Sube solo con cambios incompatibles. Añadir un campo opcional no la sube.
- `subscribe` devuelve `Hello { protocolVersion, appVersion }`. Si no coincide (posible con HMR en dev o con un futuro `Transport` por WebSocket), la UI muestra un aviso y no aplica los diffs.
- Cada `Diff` lleva un `seq` monotónico. Si la UI detecta un hueco, pide un `Snapshot` nuevo.
- Mensajes con etiqueta (`#[serde(tag = "type")]`): `Snapshot`, `Diff`, `Metrics`. Errores tipados: `IpcError { code, message }`.
- HTTP de hooks: la versión va en la ruta, `/v1/hooks/{provider}`. Los payloads de los CLIs se leen de forma tolerante: solo los campos conocidos y sin `deny_unknown_fields`.

Los tipos del protocolo viven en un crate aparte, `crates/protocol` (solo `serde` y `ts-rs`), para que la generación sea rápida y no compile el core. **Confirmado en la revisión:** `kiti-core` depende de `kiti-protocol` solo desde `application::projection`; el dominio no lo importa y el forwarder tampoco (envía bytes crudos).

### Protocolo v1 (forma mínima)

Con `#[serde(rename_all = "camelCase")]`. Es lo que necesita el Hito 1:

```rust
pub const PROTOCOL_VERSION: u32 = 1;
pub struct AgentId(pub String);                 // "{provider}:{pid}:{start_ms}", opaco para la UI
pub enum Provider { Claude, Codex, Gemini }      // "claude" | "codex" | "gemini"
pub enum StatusKind { Working, Waiting, Idle, Error, Dead }
pub enum StatusReason { Hook, SessionFile, Stale, Failure, ProcessGone, Startup }
pub struct Status { kind: StatusKind, reason: StatusReason, since: u64 }
pub struct AgentState { id: AgentId, provider: Provider, pid: u32, cwd: String,
                        started_at: u64, status: Status, last_tool: Option<String>, last_event_at: u64 }
pub struct Hello { protocol_version: u32, app_version: String, source: Source /* Live | Sim */ }
#[serde(tag = "type")]
pub enum ServerMsg { Snapshot { seq: u64, agents: Vec<AgentState> },
                     Diff { seq: u64, ingest_ms: u64, upserts: Vec<AgentState>, removed: Vec<AgentId> },
                     Metrics(CoreMetrics) }
pub enum KillResult { Killed, Cancelled, StaleProcess, NotFound, Failed { message: String } }
pub struct UiMetrics { /* p50/p95 de frame_ms y diff_to_paint_ms, fps, frames en reposo, oficinas visibles */ }
pub struct IpcError { code: String, message: String }
```

Los slots y el nombre visible (último segmento del `cwd`) los calcula la UI, no el protocolo.

## 2. Seguridad

### Capabilities, permissions y CSP

En Tauri 2.12 los comandos de la app **solo pasan por el ACL si la app define un `AppManifest`** (`has_app_acl_manifest` en `webview/mod.rs`). Si no, cualquier comando registrado es accesible. Por eso se declaran en `build.rs`:

```rust
tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
    tauri_build::AppManifest::new()
        .commands(&["subscribe", "kill_agent", "report_ui_metrics", "set_debug_stream"]),
)).expect("tauri build");
```

```json
{ "identifier": "main", "windows": ["main"],
  "permissions": ["allow-subscribe", "allow-kill-agent", "allow-report-ui-metrics",
                  "allow-set-debug-stream", "log:allow-log"] }
```

No se incluye `core:default`, que da acceso a menús, tray, imágenes y paths desde JS. La ventana, el tray y la barra de menú se gestionan desde Rust. El `fetch` interno de los channels está exento del ACL, así que no requiere permiso. Un test de humo del scaffold debe confirmarlo.

```json
"security": {
  "csp": { "default-src": "'self'", "script-src": "'self'", "style-src": "'self'",
           "img-src": "'self' data: blob:", "worker-src": "'self' blob:",
           "connect-src": "ipc: http://ipc.localhost", "object-src": "'none'",
           "base-uri": "'none'", "frame-src": "'none'", "form-action": "'none'" },
  "devCsp": { "...": "igual + connect-src ws://localhost:5173 para el HMR de Vite" },
  "freezePrototype": true
}
```

Sin contenido remoto ni CDN, `withGlobalTauri: false` y `assetProtocol` desactivado. El patrón *isolation* no se adopta en el MVP porque no se carga contenido de terceros. Consecuencias para la UI, resueltas en la revisión:

- **Pixi v8** usa `new Function` salvo que se importe `pixi.js/unsafe-eval`; la UI lo importa, así que la CSP no necesita `'unsafe-eval'`.
- **React** pone los estilos por CSSOM, que la CSP no bloquea. **Radix `ScrollArea`** inyecta un `<style>` y por eso no se usa. En producción, Vite extrae el CSS a archivos.
- **`devCsp`** añade `'unsafe-inline'` en `style-src`, porque Vite inyecta el CSS con `<style>` durante el HMR. Solo en desarrollo.

| Plugin | Decisión | Motivo |
|---|---|---|
| `tauri-plugin-log` 2.10 | Incluir como puente de logs JS | Ver §3 |
| `tauri-plugin-dialog` 2.8 | Incluir, **solo la API Rust** y sin permisos JS | Confirmación nativa del kill |
| `tauri-plugin-single-instance` 2.5 | Incluir | Una sola instancia del HookServer y del puerto |
| `tauri-plugin-shell` | **Evitar** | El kill va por `sysinfo` en Rust. No se ejecuta nada |
| `tauri-plugin-fs`, `-http` | **Evitar** | La UI no lee archivos ni usa la red |
| `tauri-plugin-opener` | Después del MVP, con scope | "Abrir cwd en Finder" |
| `tauri-plugin-updater` 2.14 | Solo documentado | §4 |
| `tauri-plugin-devtools` (CrabNebula) | Evitar en el MVP | Choca con otro logger global |

La app se distribuye fuera de la Mac App Store: el App Sandbox impediría leer el cwd de otros procesos y enviarles señales.

### Servidor de hooks

| Control | Decisión |
|---|---|
| Bind | `127.0.0.1:<puerto fijo configurable>` (nunca `0.0.0.0`). Las URLs usan `127.0.0.1` literal para que `localhost` no resuelva a `::1`. Si el puerto está ocupado: error visible y métrica |
| Token | 32 bytes de CSPRNG (`getrandom`) en base64url, comparados en tiempo constante (`constant_time_eq`). Cabecera `Authorization: Bearer` |
| Almacenamiento | **Archivo `0600`** en `~/Library/Application Support/<bundle-id>/hook-token`, creado con `O_EXCL`. Se verifica el modo al arrancar |
| Host | Debe ser exactamente `127.0.0.1:<puerto>`. Si no, 421. Corta el DNS rebinding |
| Origin | Se rechaza cualquier petición con `Origin` o `Sec-Fetch-*` (un CLI no los envía; un navegador sí) |
| Content-Type | Se exige `application/json`, lo que obliga a un preflight CORS que nunca se responde |
| Body | `DefaultBodyLimit` de 4 MiB, con el histograma `hook_body_bytes` para ajustarlo (`PostToolUse` puede traer contenidos grandes) |
| Tiempo | `TimeoutLayer` de 2 s y `ConcurrencyLimitLayer` de 64. En la config de Claude, `timeout: 2` (su valor por defecto en HTTP es 600 s) |
| Respuesta | Siempre `204` vacío. Kiti **nunca** devuelve decisiones (`permissionDecision`) ni influye en el agente |

Se descarta el keychain (`keyring` 4.2): Claude Code necesita el secreto en claro en su hook HTTP (literal en `~/.claude/settings.json`, porque la interpolación `$VAR` exige exportarlo en la shell del usuario) y el forwarder también lo lee. El token protege de navegadores y de otros usuarios, no de malware del mismo usuario. Además, el ACL del keychain pediría permiso en cada rebuild sin firmar. Habrá una acción "Regenerar token" que reescribe el archivo y los hooks.

### Kill de procesos

1. Al detectar un agente, el core guarda `ProcessIdentity { pid, start_time, uid, cmd_fingerprint }`.
2. `kill_agent(agent_id)` acepta solo un id del registro, **nunca un PID que llegue de la UI**. Refresca ese PID y verifica que `start_time`, `uid` (el usuario actual) y la huella de cmd coincidan. Si no, `IpcError::StaleProcess`. Rechaza PID ≤ 1 y el PID propio.
3. Confirmación **nativa** desde Rust (`tauri-plugin-dialog`, async), con provider, cwd y PID. Un XSS no puede saltársela. Para no pedir dos confirmaciones seguidas, `TauriTransport` declara `killConfirmation: "native"` y la UI no muestra su `AlertDialog`; en el simulador (`"in-app"`) sí lo muestra.
4. Vuelve a verificar (el diálogo tardó), envía SIGTERM, espera 5 s, verifica otra vez y envía SIGKILL. macOS no tiene pidfd, así que la verificación justo antes de cada señal reduce la carrera a microsegundos.
5. Registra un evento `audit` en `info`. En Windows (más adelante): `taskkill /T /PID <n>` con `Command` y argumentos fijos, nunca con una shell.

### Payloads como datos

- Nada de un payload se pasa a una shell, a `Command`, a `open` ni a una URL.
- `transcript_path` se canonicaliza y tiene que estar bajo `~/.claude/projects`, `~/.codex/sessions` o `~/.gemini/tmp`.
- Las cadenas se truncan (p. ej. 256 caracteres) antes de llegar a la UI, que las pinta como texto (Pixi `Text` o JSX), nunca con `innerHTML`.
- Los cuerpos de los payloads **no se registran en los logs**: contienen prompts y código. Se registran solo metadatos (evento, herramienta, sesión y tamaño).

### Supply chain

- **`cargo-deny`** (advisories de RustSec, licencias, bans y sources) en cada PR y en un cron semanal. Cubre lo mismo que `cargo-audit`, así que este no se usa en CI. `deny.toml`: licencias permisivas (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode-3.0, MPL-2.0), sources solo de crates.io y `unknown-git = "deny"`.
- **pnpm 12**: `pnpm audit --audit-level high` en CI, `minimumReleaseAge: 1440` (el valor por defecto actual), `trustPolicy: no-downgrade` y scripts de instalación solo en `allowBuilds` (la opción que recomienda la documentación actual de pnpm). La versión se fija en `mise.toml` y en `packageManager` de `package.json`.
- `Cargo.lock` y `pnpm-lock.yaml` versionados. En CI, `--locked` y `--frozen-lockfile`.
- Acciones de GitHub fijadas por SHA, con `permissions: contents: read` por defecto.
- **Política**: cada dependencia nueva se justifica en el PR (mantenimiento en los últimos 12 meses, licencia, alternativa en std o en lo que ya existe). Se prefieren los plugins oficiales `tauri-apps`.

## 3. Observabilidad (ODD)

### Logs: `tracing` directo

**Decisión: `tracing` + `tracing-subscriber` + `tracing-appender`** como pipeline único. `crates/core` no depende de Tauri, así que no puede usar el plugin. `tauri-plugin-log` se usa **solo como puente** para que los logs de JS lleguen al backend: con `skip_logger()` y **sin** su feature `tracing`, porque esa feature emite el evento en `log` y en `tracing`, lo que lo duplicaría con `LogTracer`. Los registros de `log` de Tauri y de la UI entran a `tracing` vía `tracing-log` (activo por defecto en `.init()`).

```rust
let file = RollingFileAppender::builder()
    .rotation(Rotation::DAILY).filename_prefix("kiti").filename_suffix("jsonl")
    .max_log_files(7).build(app_log_dir)?;          // ~/Library/Logs/<bundle-id>/
let (nb, _guard) = tracing_appender::non_blocking(file);
tracing_subscriber::registry()
    .with(EnvFilter::try_from_env("KITI_LOG").unwrap_or_else(|_| "info".into()))
    .with(fmt::layer().json().with_writer(nb))
    .with(cfg!(debug_assertions).then(|| fmt::layer().pretty()))
    .init();
```

| Nivel | Uso |
|---|---|
| `error` | Fallo que el usuario nota: puerto ocupado, kill fallido |
| `warn` | Peticiones de hook rechazadas, PID obsoleto, `cwd` sin leer |
| `info` | Ciclo de vida: arranque, agente aparece o muere, kill (audit), resumen de métricas cada 60 s |
| `debug` | Cada evento de hook (solo metadatos), ticks del scanner, batches IPC |
| `trace` | Diffs completos. Solo en local |

Spans: `agent{agent_id, provider, pid}` como padre de `session{session_id}`, y además `hook{event, provider}`, `scanner.tick`, `tailer.read{agent_id, bytes}` e `ipc.flush{seq, upserts, removed}`. Las tareas por agente se instrumentan con `.instrument(span)`.

### Métricas internas

Un struct `Telemetry` en el core con `AtomicU64` y `hdrhistogram` (p50, p95 y p99). No se usa la fachada `metrics`, que solo compensa si se exporta a Prometheus u OTel.

| Métrica | Origen | Objetivo |
|---|---|---|
| `events_total{source=hook\|scanner\|tailer, provider}` y eventos/s | Core | Informativa |
| `hook_rejected_total{reason=host\|origin\|auth\|size}` | HookServer | 0 en uso normal |
| `hook_to_send_ms` (recepción → `Channel::send`) | Core | p95 < 120 ms (incluye la coalescencia de 100 ms) |
| `hook_to_paint_ms` (`ingestMs` del diff → frame pintado) | UI | p95 < 200 ms |
| `frame_ms` (CPU de `renderer.render`) y fps al animar | UI | p95 < 8 ms con 20 agentes |
| `ipc_batch_bytes`, `scanner_tick_ms`, `tailer_bytes_total` | Core | Detectar regresiones |
| RSS y CPU propios | `sysinfo` sobre el PID propio, en el tick de 2 s | CPU < 1 % en reposo |

La UI envía `report_ui_metrics` cada 2 s con un resumen agregado, nunca por frame. Limitación: el proceso `WebContent` de WKWebView es un servicio XPC fuera del árbol de procesos y WebKit no expone `performance.memory`. La RAM total se mide aparte con Activity Monitor o `footprint` en los benchmarks con 20 y 50 agentes simulados.

### OpenTelemetry

**Decisión: no en el MVP.** Todo es local y en un solo proceso, y los crates de OTel siguen en 0.x con cambios frecuentes. Como la instrumentación ya está en `tracing`, añadir después una feature de cargo `otel` desactivada por defecto (`tracing-opentelemetry` + `opentelemetry-otlp` hacia un Jaeger local) es barato. Para perfilar se usa Instruments de Xcode.

### Panel de debug

Un overlay de React (`Cmd+Shift+D`), disponible también en release como opción oculta, que muestra: eventos/s por fuente, p95 de `hook_to_paint_ms` y `frame_ms`, fps, agentes y oficinas visibles, RSS y CPU, rechazos por motivo, tamaño del batch, `PROTOCOL_VERSION` y la ruta del log. Recibe un mensaje `Metrics` a 1 Hz por el mismo channel, y solo con el panel abierto (`set_debug_stream(true)`), para no pagar nada en reposo.

### Cómo guía el desarrollo

Cada hito empieza con una **ficha de observabilidad** en el issue o PR, antes de escribir el código. El Hito 1 no tiene backend: su ficha usa solo las métricas de la UI (`frame_ms`, `diff_to_paint_ms`, frames en reposo, oficinas visibles), leídas con `window.__kiti.metrics()` y en el panel de debug alimentado localmente (ADR 0006). Pasos de la ficha:

1. Preguntas que tiene que poder responder (p. ej. Hito 4: "¿cuánto tarda un `PreToolUse` en poner la sala en verde?").
2. Señales con nombre: span, métrica y log.
3. Umbral de aceptación (de la tabla anterior).
4. Verificación: un test con `tracing-test` que confirma que el span o la métrica existen, más una lectura en el panel con el generador de 20 agentes.

El PR no se cierra hasta que la señal se vea en el panel.

## 4. Tooling del monorepo

```
kiti/
  Cargo.toml            # [workspace] members = ["crates/*", "src-tauri"], resolver = "3"
  Cargo.lock  rust-toolchain.toml  deny.toml  .cargo/config.toml
  package.json          # privado, solo devDeps de tooling (commitlint, lefthook); packageManager pnpm@12.15.0
  pnpm-workspace.yaml   # packages: ["ui"]
  pnpm-lock.yaml  mise.toml  lefthook.yml  biome.json  commitlint.config.js  renovate.json
  .github/workflows/ci.yml  .github/workflows/release.yml (documentado, sin activar)
  fixtures/sim/         # perfiles y conformidad compartidos por los dos simuladores
  crates/{core,protocol,forwarder}  src-tauri/  ui/
```

| Tema | Decisión | Motivo |
|---|---|---|
| Task runner | **`mise`** (`[tools]` + `[tasks]`) | Una sola herramienta fija las versiones (`node = "24.22.1"`, `pnpm = "12.15.0"`, `cargo-nextest`, `cargo-deny`) y define tareas (`gen-types`, `ui:dev`, `ui:shots`, `ui:test`, `ui:e2e`, `check`). `just` solo haría tareas y los scripts de pnpm no cubren Rust. Rust se fija con `rust-toolchain.toml` (rustup, `channel = "1.99.1"`); el MSRV (`rust-version = "1.95"`) va en `Cargo.toml`. Node 24 cumple lo que piden Vite 8 (`^20.19 \|\| >=22.12`) y Vitest 5 (`^22.12 \|\| ^24 \|\| >=26`) |
| Hooks de git | **`lefthook`** 2.x | Binario único, paralelo, sin depender de husky. `pre-commit`: `cargo fmt --check`, `biome check --staged` y el anonimizador de fixtures. `commit-msg`: commitlint. `pre-push`: clippy, `tsc --noEmit` y los tests rápidos |
| Commits | Conventional Commits con `@commitlint/config-conventional` | Changelog y versionado futuros |
| Dependencias | **Renovate** | Agrupa `tauri` (crate), `@tauri-apps/*` y los plugins en un solo PR, porque sus minors tienen que ir alineadas. Tiene `minimumReleaseAge` (3 días), mantenimiento del lockfile y fija los digests de las acciones. Dependabot no agrupa entre ecosistemas |

### CI (GitHub Actions)

| Job | Runner | Pasos |
|---|---|---|
| `lint` | `ubuntu-latest` | `cargo fmt --check`, `cargo-deny-action@v2`, `pnpm audit`, `biome ci`, `tsc --noEmit`, `mise run gen-types` + `git diff --exit-code` |
| `core` | `ubuntu-latest` | `cargo nextest run -p kiti-core -p kiti-protocol -p kiti-forwarder --locked` y `cargo build -p kiti-core --no-default-features` (desde el Hito 2) |
| `ui` | `ubuntu-latest` | Vitest con cobertura y Playwright **funcional** con el proyecto `chromium` (sin capturas) contra el build de Vite, con el simulador |
| `ui-macos` | `macos-latest` | Playwright con los proyectos `webkit` (funcional) y `webkit-visual` (capturas `-webkit-darwin`). En el Hito 1 es **informativo** (`continue-on-error`), porque las baselines se fijan en el Mac del usuario y el runner puede pintar distinto; pasa a obligatorio si se mantiene estable dos semanas |
| `app` | `macos-latest` | Desde el Hito 2: `cargo clippy --workspace --all-targets --locked -- -D warnings`, tests de `src-tauri` y `tauri build --debug --no-bundle` |

Las capturas no se comparan en Ubuntu: WebKit en Linux no es el motor de WKWebView y sus píxeles no coinciden con los de macOS. El rendimiento (FPS) se mide solo en local, en modo *headed*.

La caché usa `Swatinem/rust-cache@v2` y `setup-node` con `cache: pnpm`, y las herramientas se instalan con `taiki-e/install-action`. Los jobs de macOS cuestan unas 10 veces más minutos, así que se limitan a lo que necesita WKWebView o AppKit. El E2E sobre la app real (WebdriverIO con `tauri-plugin-wdio-webdriver`, que da soporte a macOS, cosa que `tauri-driver` no hace) queda fuera del MVP. **Qué se prueba en cada capa lo deciden los ADRs de tests**; este ADR solo fija dónde y cómo corre.

### Empaquetado (solo documentado para el MVP)

- **Firma**: certificado *Developer ID Application* (Apple Developer Program), `APPLE_CERTIFICATE` (.p12 en base64), `APPLE_CERTIFICATE_PASSWORD` y `APPLE_SIGNING_IDENTITY`. El hardened runtime ya viene activo por defecto (`bundle.macOS.hardenedRuntime`). No hacen falta entitlements especiales.
- **Notarización**: con una clave de la App Store Connect API (`APPLE_API_ISSUER`, `APPLE_API_KEY`, `APPLE_API_KEY_PATH`), preferida a Apple ID y contraseña de app.
- **Updater**: `tauri signer generate`, `TAURI_SIGNING_PRIVATE_KEY` como secret, `pubkey` en la config, `bundle.createUpdaterArtifacts: true` y un `latest.json` en GitHub Releases generado por `tauri-apps/tauri-action`. El permiso es `updater:default`, pero la comprobación se lanza desde Rust y no desde JS.
- Mientras tanto, las builds locales usan firma ad hoc (`signingIdentity: "-"`).

## Consecuencias

- Los 4 wrappers de los comandos en TS se mantienen a mano, y CI verifica los tipos generados.
- Sin `core:default`, cada API de ventana que pida la UI necesita su permiso explícito, con su motivo.
- El token en claro en `~/.claude/settings.json` es una concesión aceptada por el modelo de amenazas.

## Dependencias con otros ADRs

Resueltas en la revisión (ADR 0006), salvo lo indicado:

- **Dominio de `core`**: `crates/protocol` confirmado; `AgentState` y `ServerMsg` arriba; `ProcessIdentity` en el agregado `Agent`; `Telemetry` en `application::telemetry`.
- **Arquitectura de la UI**: `Transport` con `connect(onMsg)` y `killConfirmation`; resync con `connect()`; `'unsafe-inline'` solo en `devCsp`.
- **Estrategia de tests**: jobs arriba. El E2E con WebdriverIO sigue fuera del MVP.
- **Hooks y forwarder** (Hitos 4 y 5): rutas `/v1/hooks/{provider}` confirmadas. **Pendiente:** el puerto por defecto, que se fija en el Hito 4.

## Cuándo reabrir esta decisión

- `tauri-specta` publica la 2.0 estable, o los comandos pasan de ~10.
- Aparece un flujo entre procesos que no se puede depurar sin trazas distribuidas (OTel).
- Kiti carga contenido remoto (isolation pattern y revisión de la CSP).
- Claude Code permite leer el secreto de la cabecera desde un archivo o el keychain.

## Fuentes

- Channels frente a events: https://v2.tauri.app/develop/calling-frontend/
- Permissions y capabilities: https://v2.tauri.app/security/permissions/ y https://v2.tauri.app/reference/acl/core-permissions/
- `AppManifest::commands`: https://docs.rs/tauri-build/latest/tauri_build/struct.AppManifest.html
- Comprobación del ACL y exención de channels: https://github.com/tauri-apps/tauri/blob/dev/crates/tauri/src/webview/mod.rs
- CSP: https://v2.tauri.app/security/csp/
- Firma en macOS: https://v2.tauri.app/distribute/sign/macos/
- Updater: https://v2.tauri.app/plugin/updater/
- Logging: https://v2.tauri.app/plugin/logging/ y el código de `tauri-plugin-log` 2.10.1 (`src/commands.rs`, `src/lib.rs`): https://crates.io/crates/tauri-plugin-log
- Testing: https://v2.tauri.app/test/ y https://webdriver.io/docs/desktop-testing/tauri/platform-support
- ts-rs: https://github.com/Aleph-Alpha/ts-rs y https://crates.io/crates/ts-rs
- tauri-specta: https://github.com/specta-rs/tauri-specta y https://crates.io/crates/tauri-specta
- Hooks HTTP de Claude Code: https://code.claude.com/docs/en/hooks
- CrabNebula devtools (conflicto de logger): https://github.com/crabnebula-dev/devtools
- tracing-appender: https://docs.rs/tracing-appender
- cargo-deny: https://embarkstudios.github.io/cargo-deny/ y https://github.com/EmbarkStudios/cargo-deny-action
- pnpm: https://pnpm.io/supply-chain-security
- Renovate: https://docs.renovatebot.com/key-concepts/minimum-release-age/
- lefthook: https://github.com/evilmartians/lefthook
- mise: https://mise.jdx.dev y https://github.com/jdx/mise-action
- Caché de Rust: https://github.com/Swatinem/rust-cache
- tauri-action: https://github.com/tauri-apps/tauri-action

## Revisión del arquitecto líder (2026-10-24)

Estado: **Aceptada con cambios.** Detalle en el [ADR 0006](../0006-arquitectura-consolidada/README.md).

1. **`crates/protocol` confirmado** frente a `core::application::protocol` del ADR 0002: compila en segundos, no arrastra adapters y permite generar los tipos en el Hito 1 sin backend. Se añade la forma mínima del protocolo v1.
2. **ts-rs:** los tipos se generan con `mise run gen-types`, fuera del build, y se versionan. Verificado que ts-rs 12 admite `TS_RS_EXPORT_DIR` y `TS_RS_LARGE_INT` (este último desde la 12.0.0).
3. **CI:** las capturas solo en macOS (`ui-macos`, informativo en el Hito 1); en Ubuntu, Vitest y Playwright funcional con Chromium; el rendimiento solo en local. El job `app` empieza en el Hito 2.
4. **Versiones fijadas:** Node 24.22.1 LTS, pnpm 12.15.0 (no 11: la 12 es la actual), Rust 1.99.1 en `rust-toolchain.toml` con MSRV 1.95.
5. **pnpm:** `allowBuilds` en lugar de `onlyBuiltDependencies`. El valor por defecto de `minimumReleaseAge` (1440) está confirmado; no está confirmado desde qué versión.
6. **CSP:** `pixi.js/unsafe-eval`, sin `ScrollArea` de Radix y `'unsafe-inline'` solo en `devCsp`.
7. **Kill:** una sola confirmación (`killConfirmation`).
8. La ficha de observabilidad es obligatoria desde el Hito 1, con las métricas de la UI.

