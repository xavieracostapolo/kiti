# ADR 0002: Diseño DDD del backend Rust, crates y estrategia TDD

- **Fecha:** 2026-10-24
- **Estado:** Aceptada con cambios (ver la revisión al final)
- **Decisión:** `crates/core` es un único crate con capas `domain` / `application` / `ports` / `adapters`. Los adapters van detrás de *features* de Cargo, así que el compilador garantiza la regla de dependencias. El semáforo es un reducer puro sobre el agregado `Agent`. Los ports usan `async fn` nativo con genéricos, sin `async-trait`. En los tests se usan fakes en lugar de mocks. El forwarder usa `ureq` sin TLS. El simulador de agentes es determinista: usa ChaCha8 con semilla. Los DTOs hacia la UI viven en el crate `crates/protocol` (ADR 0004 y 0006), no en `core`.

## Contexto

El ADR 0001 fijó el stack: Tauri v2, Rust con `tokio` y TS/PixiJS, primero macOS y después Windows. `AGENTS.md` añade tres reglas: `crates/core` no depende de Tauri, los tipos del protocolo viven en Rust y el rendimiento con 20–50 agentes es la prioridad número uno. Este ADR define cómo se organiza y se prueba el backend sin reinventar piezas que ya existen. Solo se usan crates maduros y mantenidos, y sin advisories abiertos en RustSec.

## Decisión

### 1. Bounded contexts y lenguaje ubicuo

El dominio es pequeño, así que los contextos son **módulos** y no crates.

| Contexto | Responsabilidad | Términos |
|---|---|---|
| **Monitoring** (núcleo) | Estado de cada agente y su semáforo | `Agent`, `Session`, `Status`, `TrafficLight`, `Observation`, `DomainEvent` |
| **Ingestion** (capa anticorrupción) | Traducir los payloads de cada CLI (hooks, líneas JSONL, procesos) a `Observation` canónicas | `Provider`, `HookEvent`, `TranscriptLine`, `ProcessSnapshot` |
| **Office** | **Fuera del core.** La proyección visual y la asignación de *slots* estables son de la UI (`ui/src/domain/layout.ts`, ADR 0003) | — |
| **Control** | Cerrar un agente después de que el usuario lo confirme | `CloseOffice` (comando), `KillTarget`, `KillOutcome` |

Glosario mínimo:

- **Agent:** un proceso de CLI de código vivo y clasificado (Claude, Codex o Gemini).
- **Session:** una conversación del CLI, identificada por `session_id`. Un agente puede encadenar varias, por ejemplo con `/clear`.
- **Observation:** un hecho de entrada ya normalizado.
- **DomainEvent:** un hecho de salida que produce el reducer.
- **TrafficLight:** el color derivado de `Status`. No se guarda nunca: se calcula.

### 2. Modelo de dominio

**Agregado raíz `Agent`.** Su identidad es `AgentId = (Provider, Pid, ProcessStartTime)`. Incluir el `start_time` evita confundir un PID reutilizado por el sistema. En el protocolo se serializa como una cadena opaca `"{provider}:{pid}:{start_ms}"`. Para el kill, el agregado guarda además `ProcessIdentity { pid, start_time, uid, cmd_fingerprint }` (ADR 0004). Dentro del agregado viven la entidad `Session` (activa o histórica), `pending_tools: SmallMap<ToolUseKey, ToolName>`, `last_event` y `last_tool`.

**Value objects** (inmutables y validados en el constructor): `Provider`, `Pid`, `ProcessStartTime`, `SessionId`, `WorkingDir`, `ToolName`, `TranscriptPath`, `Timestamp` (ms en `u64`, que siempre viene del port `Clock`; el dominio nunca lee el reloj) y `Status { kind, reason, since }`.

**Servicios de dominio puros:**

- `classify(exe, argv) -> Option<(Provider, Role)>`: detecta el CLI por `exe` y `argv`, nunca por el nombre del proceso (Gemini aparece como `node`). `Role` es `Agent` o `Launcher`, porque Codex con npm y Gemini lanzan **dos procesos** por sesión (ADR 0005 §4). `dedupe_tree(snapshots)` se queda con el proceso clasificado más profundo de cada cadena del mismo provider. Es el **único** sitio donde se clasifica: el forwarder no clasifica.
- `Correlator`: enlaza un `SessionId` con su `AgentId`, por orden de fiabilidad:
  1. **Claude:** la observación `SessionFile` que sale de `~/.claude/sessions/<pid>.json` (`pid` ↔ `sessionId`).
  2. **Codex y Gemini:** la cabecera `X-Kiti-Ppid` del forwarder (su `getppid()`). El core sube por los ancestros con `ProcessScanner::ancestors(pid)` hasta el primer proceso clasificado como `Agent` del mismo provider.
  3. **Respaldo:** `(provider, cwd)` si hay un único candidato sin sesión; si hay varios, la hora de inicio del proceso frente al primer evento, con ±5 s.
  Una sesión sin proceso se retiene 5 s (y fuerza un escaneo inmediato) antes de descartarla con la métrica `correlation_failed_total`, porque solo se muestran agentes vivos.
- `AgentRegistry`: la colección de agregados. Enruta las observaciones y produce `Vec<DomainEvent>`.

**Observation (entrada):**

- `ProcessSeen(ProcessSnapshot { pid, ppid, start_time, uid, exe, argv, cwd })` y `ProcessGone { pid, start_time }`.
- `Signal { provider, session_id, pid_hint, cwd, at, signal }`: un hook ya normalizado por la ACL a uno de los **8 eventos de Kiti** del ADR 0005 §5. Para no chocar con los nombres de salida, en el código son `Signal::{SessionStart { source }, PromptSubmit, ToolStart { key, tool }, ToolEnd { key, outcome: Ok | Failed | Denied }, AwaitingInput { cause }, Stop { cause: Done | Interrupted | IdlePrompt }, Failure { kind }, SessionEnd { reason }}`, más `Unknown { name }`, que solo cuenta una métrica.
- `SessionFile { pid, session_id, status: Busy | Idle }`: solo Claude, desde `~/.claude/sessions/<pid>.json`.
- `TranscriptActivity { session_id, last_tool }`, `Tick(Timestamp)`, `KillRequested(AgentId)` y `KillCompleted { id, outcome }`.

**DomainEvent (salida):** `AgentAppeared`, `SessionStarted`, `StatusChanged { from, to, reason, at }`, `ToolStarted`, `ToolFinished`, `AgentExited { expected }`, `AgentRemoved`.

### 3. Máquina de estados del semáforo (dominio puro)

```rust
impl Agent {
    /// Puro y determinista: sin I/O, sin reloj, sin aleatoriedad.
    pub fn apply(&mut self, obs: &Observation, now: Timestamp) -> SmallVec<[DomainEvent; 2]>;
    pub fn light(&self) -> TrafficLight { self.status.kind.into() }
}
```

Tabla única, alineada con el mapeo del ADR 0005 §5. Las mismas filas están en `fixtures/sim/conformance/*.json`, que también ejecuta el simulador de la UI (ADR 0006).

| Desde | Señal | Hacia |
|---|---|---|
| cualquiera viva | `SessionStart` | `Idle` (gris); se vacía `pending_tools` y se olvida un `SessionEnd` anterior |
| `Idle` / `Waiting` / `Error` | `PromptSubmit`, `ToolStart` | `Working` (verde) |
| `Working` / `Waiting` | `ToolEnd` (cualquier `outcome`) | `Working`; se quita la tool de `pending_tools`. Un `ToolEnd` sin su `ToolStart` se ignora. **Un fallo de herramienta no es rojo** |
| `Working` | `AwaitingInput` (`PermissionRequest`, `Notification` de permiso o elicitación) | `Waiting` (amarillo) |
| `Working` / `Waiting` | `Stop` (cualquier `cause`, incluido `idle_prompt`) | `Idle`; se vacía `pending_tools` |
| `Working` / `Waiting` | `SessionFile(Idle)` (solo Claude) | `Idle` con `reason = SessionFile`. Cubre la interrupción con Esc, que no dispara `Stop` |
| `Idle` | `SessionFile(Busy)` | `Working` con `reason = SessionFile` |
| cualquiera viva | `Failure` (`StopFailure`) | `Error` (rojo). Sale con el siguiente `PromptSubmit` |
| cualquiera viva | `SessionEnd` | `Idle`; marca la salida como esperada hasta el siguiente `SessionStart` |
| cualquiera viva | `ProcessGone` con `SessionEnd` o `KillRequested` previos | se elimina (`AgentExited { expected: true }` y `AgentRemoved`) |
| cualquiera viva | `ProcessGone` sin causa | `Dead` (rojo; absorbente, se retira tras un TTL de 60 s) |
| `Working` | `Tick` sin señales durante 30 s y `pending_tools` vacío | `Idle` con `reason = Stale` |
| `Working` | `Tick` sin señales durante 10 min con herramientas abiertas | `Idle` con `reason = Stale` (una herramienta puede tardar mucho) |
| `Waiting` | `Tick` | Nunca caduca: el usuario puede tardar |

Los umbrales viven en `StalePolicy { idle_after: 30 s, pending_max: 10 min, dead_ttl: 60 s }`. Los hooks mandan. `SessionFile` es la segunda fuente para Claude. El transcript solo enriquece `last_tool` y sirve de respaldo cuando no hay hooks configurados. Los nombres de evento de los tres CLIs están verificados en el ADR 0005.

### 4. Ports (traits)

Se usa `async fn` nativo en traits (estable desde 1.75) con **despacho estático por genéricos**. `dyn` no es compatible con traits async ([Rust goals 2026](https://rust-lang.github.io/goals/2026/afidt-box.html)), pero no lo necesitamos. **No se usa `async-trait`.**

```rust
pub trait Clock: Send + Sync { fn now(&self) -> Timestamp; }

/// Síncrono: sysinfo es bloqueante. El runtime lo llama dentro de spawn_blocking cada 2 s.
pub trait ProcessScanner: Send + 'static {
    fn scan(&mut self) -> Result<Vec<ProcessSnapshot>, ScanError>;
    /// Refresco puntual de un PID y sus ancestros, para resolver `X-Kiti-Ppid`.
    fn ancestors(&mut self, pid: Pid) -> Result<Vec<ProcessSnapshot>, ScanError>;
}

pub trait HookSource: Send + 'static {
    fn run(self, tx: ObservationTx) -> impl Future<Output = Result<(), SourceError>> + Send;
}
pub trait TranscriptSource: Send + 'static {
    fn run(self, tx: ObservationTx, watch: WatchRx) -> impl Future<Output = Result<(), SourceError>> + Send;
}
/// Solo Claude: vigila `~/.claude/sessions/` y emite `Observation::SessionFile`.
pub trait SessionFileSource: Send + 'static {
    fn run(self, tx: ObservationTx) -> impl Future<Output = Result<(), SourceError>> + Send;
}
pub trait AgentKiller: Send + Sync + 'static {
    fn terminate(&self, t: KillTarget, grace: Duration)
        -> impl Future<Output = Result<KillOutcome, KillError>> + Send;
}
/// Port de salida. `application::projection` convierte el `RegistryDiff` del dominio en
/// `kiti_protocol::ServerMsg::Diff`. Lo implementa src-tauri con un `tauri::ipc::Channel` (ADR 0004).
pub trait DiffPublisher: Send + 'static { fn publish(&mut self, msg: kiti_protocol::ServerMsg); }
```

`ObservationTx` es un `tokio::sync::mpsc::Sender<Observation>` acotado (unas 1024 posiciones). Vive en `ports`/`application`, nunca en `domain`.

`application::Runtime` es un **actor**: una única tarea es dueña del `AgentRegistry`, así que no hay `Mutex`. Hace `select!` sobre tres cosas: las observaciones, el tick del scanner y un *flush* del coalescedor cada 100 ms. En cada flush envía un único `RegistryDiff` como máximo, lo que da unos 10 Hz.

### 5. Adapters

| Adapter | Port | Notas |
|---|---|---|
| `hooks_http::AxumHookServer` | `HookSource` | Escucha en `127.0.0.1` con `POST /v1/hooks/{provider}` y un límite de body de **4 MiB**. Responde `204` **antes** de procesar: hace `try_send` y, si el canal está lleno, cuenta la pérdida y nunca bloquea al agente. Rechaza un `Host` distinto de `127.0.0.1:port` (421), cualquier petición con `Origin` o `Sec-Fetch-*` y cualquier `Content-Type` que no sea JSON. El token `Authorization: Bearer` es **obligatorio**. Los controles completos están en el ADR 0004 §2. |
| `hooks_http::acl::{claude,codex,gemini}` | (ACL) | `serde` sin `deny_unknown_fields`, para tolerar campos nuevos. Deserializa **solo la lista blanca** de campos del ADR 0005 §7 y el resto se descarta al parsear. Traduce a `Signal`. |
| `SysinfoScanner` | `ProcessScanner` | `ProcessRefreshKind::nothing().with_cmd(OnlyIfNotSet).with_cwd(OnlyIfNotSet).with_exe(OnlyIfNotSet).with_user(OnlyIfNotSet)`. `ancestors` refresca solo los PIDs pedidos (`ProcessesToUpdate::Some`) |
| `ClaudeSessionsWatcher` | `SessionFileSource` | `notify` sobre `~/.claude/sessions/` (un archivo pequeño por sesión). Parser tolerante, porque el esquema no está documentado. Si el directorio no existe, no hace nada y lo registra en `warn` |
| `NotifyTailer` | `TranscriptSource` | Vigila **solo** los `transcript_path` de las sesiones vivas, no todo `~/.claude/projects`. Lee de forma incremental desde el offset guardado, guarda la línea parcial en un buffer y detecta truncados. Parsea con `serde_json` usando `IgnoredAny` para saltarse los campos grandes. |
| `UnixKiller` / `WindowsKiller` | `AgentKiller` | En macOS: comprueba que `ProcessIdentity` (pid, start_time, uid y huella de cmd) sigue siendo la misma antes de **cada** señal, envía `kill_with(Signal::Term)` y, tras 5 s de gracia, `Signal::Kill`, todo con `sysinfo`. En Windows: `taskkill /T /PID <n>` mediante `std::process::Command` con argumentos fijos. |
| `SystemClock`, `ManualClock` | `Clock` | `ManualClock` es para los tests. |
| `sim::SimFleet` | `ProcessScanner` + `HookSource` | Ver la sección 9. |

### 6. Estructura de crates: un solo crate con *features*

```
crates/protocol/ (kiti-protocol)  # solo serde + ts-rs: DTOs, PROTOCOL_VERSION (ADR 0004)
crates/core/  (kiti-core)         # depende de kiti-protocol; el dominio no lo importa
  src/domain/{agent,session,status,observation,signal,events,classify,correlate,registry}.rs
  src/application/{runtime,coalescer,commands,projection,telemetry}.rs  # projection: dominio → DTOs
  src/ports.rs
  src/adapters/{hooks_http/,sysinfo_scanner,claude_sessions,transcripts/,kill/,clock}.rs  # cfg(feature)
  src/sim/        # feature "sim"
  src/testing/    # feature "testing": fakes reutilizables por src-tauri y las benches
  tests/fixtures/{hooks,transcripts,processes,sessions}/   tests/snapshots/   benches/
crates/forwarder/ (kiti-forwarder, binario `kiti-hook`)  # NO depende de kiti-core ni de kiti-protocol
fixtures/sim/{profiles.json,conformance/}  # compartidos con el simulador de la UI (ADR 0006)
```

Las *features* son `adapter-http` (axum), `adapter-sysinfo`, `adapter-notify`, `sim` y `testing`, y todas están activas por defecto. Las dependencias de los adapters son `optional = true`. **Si se compila `--no-default-features`, `domain` y `application` no pueden referenciar `axum`, `sysinfo` ni `notify`.** Así se obtiene la misma garantía que con crates separados, pero sin multiplicar los `Cargo.toml`, y se respeta la estructura de `AGENTS.md`. Se partirá en `kiti-domain` + `kiti-adapters` solo si aparece un segundo consumidor, por ejemplo un modo *headless*.

El dominio depende solo de `std`, `serde`, `thiserror` y `smallvec`, y **no** de `kiti-protocol`. Los DTOs son distintos de las entidades y viven en `crates/protocol` con el derive de `ts-rs` (ADR 0004). `application::projection` traduce de unos a otros. `application` y los adapters emiten `tracing` (solo la fachada); el dominio no registra nada. El *subscriber* se instala en `src-tauri`. `Telemetry` (contadores e histogramas, ADR 0004 §3) vive en `application::telemetry`.

El forwarder (`kiti-hook <provider>`) no enlaza `kiti-core` ni `kiti-protocol` y no parsea JSON:

- Lee stdin hasta 4 MiB (el mismo límite que el servidor) y hace POST de los bytes tal cual a `/v1/hooks/{provider}` con `Authorization: Bearer <token>` (leído del archivo `0600` del ADR 0004) y `X-Kiti-Ppid: <getppid()>`. Si el payload supera el límite, no envía nada.
- El plazo total es de **150 ms** (`timeout_global`). En la configuración del CLI se pone un timeout de 1 s.
- **Siempre** termina con `exit 0`. Escribe `{}` en stdout solo para Gemini, que lo exige, y nada en el resto. Falla en abierto.
- `--capture <dir>` guarda el stdin crudo para crear fixtures y no envía nada (ADR 0005 §6).
- La lista blanca de campos se aplica en la ACL del servidor, en un único sitio.

### 7. Crates seleccionados

Datos consultados el 2026-10-24 en la API de crates.io, en GitHub y en [rustsec/advisory-db](https://github.com/rustsec/advisory-db/tree/main/crates).

| Crate | Versión | Último release | Descargas | Uso | Alternativa descartada y por qué |
|---|---|---|---|---|---|
| [tokio](https://crates.io/crates/tokio) | **≥ 1.54.1** | 2026-10-21 | 1.104 M | Runtime y canales; `test-util` para el tiempo virtual | `smol`: Tauri ya usa tokio. `async-std`: discontinuado |
| [axum](https://crates.io/crates/axum) | 0.8.9 | 2026-04-14 (repo activo el 2026-10-20) | 542 M | HookServer | `hyper` directo (mucho boilerplate), `actix-web` (más pesado y con otro modelo), `tiny_http` (síncrono, un hilo extra) |
| [sysinfo](https://crates.io/crates/sysinfo) | 0.39.6 | 2026-07-09 | 230 M | Scanner de procesos y kill | `libproc` (solo macOS), `heim` (abandonado) |
| [notify](https://crates.io/crates/notify) | 8.2.1 | 2026-10-25 UTC | 180 M | FSEvents / ReadDirectoryChangesW | `notify 9.0.0-rc.6`: todavía es RC |
| [notify-debouncer-full](https://crates.io/crates/notify-debouncer-full) | 0.7.0 | 2026-01-23 | 19 M | Coalescer las escrituras en los JSONL (100–250 ms) | `debouncer-mini`: pierde el tipo de evento y no distingue los truncados |
| [serde](https://crates.io/crates/serde) / [serde_json](https://crates.io/crates/serde_json) | 1.0.229 / 1.0.154 | 2026-07-18 / 2026-10-11 | 1.584 M / 1.489 M | ACL y JSONL | `simd-json` / `sonic-rs`: usan `unsafe` y la ganancia es irrelevante con este volumen |
| [thiserror](https://crates.io/crates/thiserror) | 2.0.21 | 2026-09-23 | 1.696 M | Errores tipados por port en `core` | `snafu`: más ceremonia |
| [anyhow](https://crates.io/crates/anyhow) | 1.0.104 | 2026-07-18 | 1.086 M | Solo en el bootstrap de `src-tauri`; nunca en `core` | `eyre`: no aporta nada aquí |
| [async-trait](https://crates.io/crates/async-trait) | 0.1.92 | 2026-08-08 | 753 M | **No se usa** | `async fn` nativo con genéricos: sin `Box` por llamada |
| [ureq](https://crates.io/crates/ureq) | 3.5.0, `default-features = false` | 2026-10-21 | 241 M | Forwarder: HTTP/1.1 bloqueante sin TLS. Solo depende de `ureq-proto`, `log`, `base64`, `percent-encoding` y `utf8-zero` | `reqwest` blocking 0.13.5: crea un runtime tokio y hyper por dentro, lo que da un binario y un arranque mayores. `minreq` 3.0.0: comunidad menor (7 M). `TcpStream` a mano: sería reinventar la rueda |
| [rand](https://crates.io/crates/rand) + [rand_chacha](https://crates.io/crates/rand_chacha) | 0.10.3 + 0.10.0 | 2026-09-20 / 2026-02-02 | 1.991 M / 1.557 M | Simulador | `StdRng`: su secuencia no es reproducible entre versiones. `fastrand`: menos distribuciones |

Otras dependencias del core: `tracing` 0.1.44 (fachada), `hdrhistogram` 7.6.0 para `Telemetry` y `tower`/`tower-http` para los límites del HookServer. La tabla única de versiones aprobadas está en el ADR 0006.

Advisories relevantes (verificados el 2026-10-24 en rustsec/advisory-db y crates.io):

- [RUSTSEC-2026-0384](https://github.com/rustsec/advisory-db/blob/main/crates/tokio/RUSTSEC-2026-0384.md): carrera con *use-after-free* en `Interval` en tokio 1.54.0, versión que fue *yanked*. Por eso exigimos `>= 1.54.1`.
- [RUSTSEC-2026-0386](https://github.com/rustsec/advisory-db/blob/main/crates/tokio/RUSTSEC-2026-0386.md): `AsyncFd`, que no usamos.
- [RUSTSEC-2026-0097](https://github.com/rustsec/advisory-db/blob/main/crates/rand/RUSTSEC-2026-0097.md): `rand::rng()`, corregido en 0.10.1. Usamos ChaCha8 con semilla, no el RNG por hilo.
- Sin advisories: axum, sysinfo, notify, serde_json, ureq, insta, proptest, async-trait.

**MSRV:** `sysinfo 0.39` exige **Rust 1.95**, así que el workspace fija `rust-version = "1.95"`, con edition 2024. La toolchain de desarrollo y CI se fija aparte en `rust-toolchain.toml` con la estable actual, **1.99.1** (ADR 0004).

Perfil del forwarder, para que arranque rápido y ocupe poco. Se mide con `hyperfine` y debería quedar por debajo de unos 5 ms:

```toml
[profile.forwarder]
inherits = "release"
lto = true
codegen-units = 1
panic = "abort"
strip = true
opt-level = "s"
```

### 8. Estrategia TDD

| Capa | Herramienta | Qué se prueba |
|---|---|---|
| Dominio | Tests unitarios puros + [proptest](https://crates.io/crates/proptest) 1.12.0 (2026-10-10) | El reducer, `classify` y `Correlator`. Sin tokio |
| Application | `tokio::test(start_paused = true)` + fakes | El coalescedor (10 Hz) y los timeouts, con el tiempo virtual de `tokio::time` |
| Adapters (ACL) | Fixtures reales + [insta](https://crates.io/crates/insta) 1.49.0 (2026-10-03) con `yaml` y redacciones | De payload a `Observation` |
| Integración | `AxumHookServer` en un puerto efímero + `SimFleet` en modo *wire* | De extremo a extremo, sin CLIs reales |
| Runner | [cargo-nextest](https://crates.io/crates/cargo-nextest) 0.9.149 (2026-10-20) | Aísla cada test en su proceso y da timeouts. Los doctests van aparte con `cargo test --doc` |
| Benchmarks | [criterion](https://crates.io/crates/criterion) 0.8.2 (2026-02-04, [criterion-rs](https://github.com/criterion-rs/criterion.rs)) | `divan` 0.1.21 se descarta: su último release es del 2025-04-10 |

**Fakes en lugar de `mockall`.** Los ports son pequeños. Los fakes (`ManualClock`, `ScriptedScanner`, `RecordingKiller`, `VecPublisher`) se reutilizan en el simulador y las benches, y prueban **estado**, no secuencias de llamadas, así que son menos frágiles. `mockall` 0.15 se reconsiderará solo si un port crece mucho.

**Fixtures.** Las tablas de transición del reducer se prueban también contra `fixtures/sim/conformance/*.json` (señales de entrada y estado esperado), los mismos archivos que ejecuta el simulador de la UI. Los payloads reales se guardan en `tests/fixtures/hooks/<provider>/<version-cli>/<evento>-<n>.json` y en `tests/fixtures/transcripts/<provider>/<version-cli>/*.jsonl`. Para capturarlos se usa el modo `kiti-hook <cli> --capture <dir>` del forwarder (ADR 0005 §6). **Capturar exige ejecutar los CLIs, que cuestan dinero: solo con permiso explícito del usuario.**

Antes de commitear se pasan por el anonimizador del ADR 0005 §6: `$HOME` pasa a `/Users/USER` y los prompts, `tool_input` y respuestas se sustituyen por `"<redacted:N>"`. Los JSONL se recortan a unas 50 líneas e incluyen casos con una línea final parcial y con truncado. `sessions/*.json` guarda ejemplos de `~/.claude/sessions/<pid>.json`. `processes/*.json` guarda `exe` y `argv` reales (Gemini instalado con npm, Homebrew y npx) para probar `classify`. Cada versión nueva de un CLI añade su carpeta sin borrar las anteriores, y todos los fixtures se recorren con `insta::glob!`.

**Ejemplo test-first.** Primero se escribe el test en rojo y después el mínimo `apply` que lo hace pasar.

```rust
#[test]
fn permission_request_pone_amarillo_y_stop_vuelve_a_gris() {
    let mut a = AgentBuilder::claude().status(Status::working()).build();

    let ev = a.apply(&signal(AwaitingInput::Permission), t(10));
    assert_eq!(a.light(), TrafficLight::Yellow);
    assert_eq!(ev[..], [status_changed(Working, Waiting, t(10))]);

    a.apply(&signal(Stop::Done), t(20));
    assert_eq!(a.light(), TrafficLight::Gray);
}

proptest! {
    #[test]
    fn reducer_total_y_determinista(obs in vec(any_observation(), 0..500)) {
        let (mut x, mut y) = (AgentBuilder::claude().build(), AgentBuilder::claude().build());
        let ex: Vec<_> = obs.iter().flat_map(|o| x.apply(o, o.at())).collect();
        let ey: Vec<_> = obs.iter().flat_map(|o| y.apply(o, o.at())).collect();
        prop_assert_eq!(ex, ey);                    // determinista
        prop_assert!(!x.is_dead() || obs.iter().any(Observation::is_unexpected_exit)); // Dead solo con causa
    }
}
```

Otras propiedades que se comprueban:

- `pending_tools` nunca tiene huérfanos.
- `Dead` es absorbente.
- Un `ProcessSeen` duplicado es idempotente.
- `Stop` lleva siempre a `Idle` y deja `pending_tools` vacío.
- Un `ToolEnd { outcome: Failed }` nunca produce `Error`.

**Benchmarks** (`benches/registry.rs`), con 20 y 50 agentes:

- `AgentRegistry` procesando 10 000 observaciones de `SimFleet`.
- Cálculo de `RegistryDiff` por flush.
- Parseo de una línea JSONL real.
- `classify` sobre 500 procesos.

Objetivo orientativo: menos de 1 µs por observación y menos de 50 µs por flush con 50 agentes.

### 9. Simulador de agentes (`sim`)

```rust
pub struct FleetSpec { pub agents: u16, pub mix: ProviderMix, pub seed: u64, pub profile: Profile /* Calm | Busy | Chaos | Demo */ }

impl SimFleet {
    pub fn new(spec: FleetSpec, start: Timestamp) -> Self;
    /// Núcleo puro: avanza el tiempo virtual y devuelve observaciones. Lo usan los tests y las benches.
    pub fn step(&mut self, dt: Duration) -> Vec<Observation>;
}
impl ProcessScanner for SimFleet { /* snapshots de procesos ficticios */ }
// SimHookSource: modo `channel` (envía Observation) o modo `wire` (POST de JSON
// generado desde los fixtures contra AxumHookServer, para ejercitar la ACL y para demos).
```

- **Un RNG por agente:** `ChaCha8Rng::seed_from_u64(seed ^ index)`. Así, añadir agentes no cambia la secuencia de los demás, y ChaCha garantiza estabilidad de valores entre plataformas.
- **Cadena de Markov por agente**, a nivel de `Signal`: `Idle → PromptSubmit → (ToolStart → [AwaitingInput → espera] → ToolEnd) × k → Stop → Idle`. Las probabilidades y las medias de cada perfil se leen de `fixtures/sim/profiles.json`, el mismo archivo que usa el simulador de la UI.
  - Las duraciones son exponenciales con medias realistas (pensar 2–20 s, herramientas 0,2–30 s, espera al usuario 5–120 s).
  - El perfil `Chaos` añade `StopFailure`, caídas del proceso y hooks perdidos, para probar la política `Stale`.
- Lo usan los snapshot tests (misma semilla, mismo diff), las benches, el Hito 2 (la app Tauri con `KITI_SIM=20:42`) y las demos. El Hito 1 usa el simulador TS de la UI (ADR 0003); las dos implementaciones comparten perfiles y fixtures de conformidad, no la secuencia exacta (ADR 0006).

## Alternativas

- **Crates separados (`kiti-domain`, `kiti-app`, `kiti-adapters`):** dan aislamiento total, pero suponen más `Cargo.toml` y versionado interno para un dominio pequeño. Las *features* opcionales dan la misma garantía de compilación.
- **`async-trait` / `dyn Port`:** cuestan un `Box` por llamada y no hacen falta, porque el cableado se conoce al compilar.
- **`mockall`:** acopla los tests a la interacción, sus macros son pesadas y no se reutiliza en el simulador.
- **Estado compartido con `Arc<Mutex<Registry>>`:** introduce contención y orden no determinista. El actor se puede probar mejor.
- **`reqwest` blocking en el forwarder:** tiene más dependencias y un arranque más lento, y el forwarder corre en cada uso de herramienta.
- **`divan`:** su API es agradable, pero lleva 18 meses sin release.

## Consecuencias

- El dominio se prueba en milisegundos y sin runtime. El compilador vigila la regla de dependencias si CI compila con `--no-default-features`.
- Los cambios en los payloads de los CLIs se quedan en la ACL y se detectan con fixtures versionados.
- El simulador sirve a la vez para TDD, benches y demos, sin depender de CLIs reales.
- Coste: hay que mantener fixtures redactados por versión de CLI. El MSRV 1.95 obliga a una toolchain reciente.
- `notify 9` está en RC: habrá que migrar cuando sea estable, y el impacto queda acotado a `NotifyTailer`.

**Dependencias con otros ADRs (resueltas en la revisión):**

- **Integración IPC:** `DiffPublisher` publica `kiti_protocol::ServerMsg` por un `tauri::ipc::Channel`; tipos con `ts-rs` en `crates/protocol` (ADR 0004).
- **Observabilidad:** `core` emite `tracing` en `application` y adapters (ADR 0004 §3).
- **CI:** nextest, `cargo-deny` (sin `cargo-audit`), la compilación con `--no-default-features` y la toolchain en `rust-toolchain.toml` (ADR 0004).
- **Frontend:** los slots y el layout son de la UI. Core no sabe nada de oficinas.
- **CLIs:** Codex no admite `http` y necesita el forwarder; los eventos de Gemini y la forma de sus procesos están en el ADR 0005.

## Revisión del arquitecto líder (2026-10-24)

Estado: **Aceptada con cambios.** Lo que cambió para hacer coherentes los ADRs 0002 a 0005 (detalle en el [ADR 0006](../0006-arquitectura-consolidada/README.md)):

1. **Protocolo fuera del core.** Los DTOs pasan de `application::protocol` al crate `crates/protocol` (`kiti-protocol`, solo `serde` y `ts-rs`). `application::projection` traduce del dominio a los DTOs. `DiffPublisher` publica `ServerMsg`.
2. **Office sale del core.** Los slots y el layout son de la UI; el core no los conoce.
3. **Reducer alineado con el ADR 0005.** Las entradas son los 8 eventos normalizados (`Signal::*`, para no chocar con los `DomainEvent`), más `SessionFile` de `~/.claude/sessions/<pid>.json`. El amarillo sale de `PermissionRequest`; `idle_prompt` es gris; un fallo de herramienta no es rojo; `SessionEnd` hace esperada la salida. `StalePolicy` fija 30 s, 10 min con herramientas abiertas y 60 s de TTL para `Dead`.
4. **Correlator y clasificación.** Se añade `SessionFile` como primera fuente para Claude. El forwarder ya no recorre ancestros: envía su `getppid()` y el core sube por el árbol con `ProcessScanner::ancestors`, de modo que `classify` existe en un solo sitio. `classify` devuelve también el rol (`Agent` o `Launcher`) por los dos procesos de Codex con npm y de Gemini.
5. **HookServer y forwarder alineados con los ADRs 0004 y 0005:** ruta `/v1/hooks/{provider}`, 4 MiB, token obligatorio, `204`; forwarder `kiti-hook` con plazo de 150 ms, `{}` en stdout solo para Gemini, bytes crudos (la lista blanca se aplica en la ACL) y modo `--capture`.
6. **Kill:** verificación por `ProcessIdentity` antes de cada señal y gracia de 5 s.
7. **Datos verificados:** RUSTSEC-2026-0384 existe, afecta solo a tokio 1.54.0 (`patched >= 1.54.1`, `unaffected < 1.54.0`) y la 1.54.0 figura como *yanked* en crates.io. `sysinfo 0.39.6` declara `rust-version = 1.95`. `notify 8.2.1` se publicó el 2026-10-25 UTC; no hace falta hasta el Hito 4, cuando ya habrá superado la antigüedad mínima.
8. **Toolchain:** MSRV 1.95 en `Cargo.toml`, toolchain 1.99.1 en `rust-toolchain.toml`.
9. **Simulador:** `SimFleet` se mantiene para tests, benches y la demo en Tauri. Comparte `fixtures/sim/profiles.json` y `fixtures/sim/conformance/` con el simulador TS.

