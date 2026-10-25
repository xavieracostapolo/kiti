# ADR 0005: Contrato de integración con los CLIs de agentes

- **Fecha:** 2026-10-24
- **Estado:** Aceptada con cambios (ver la revisión al final)
- **Decisión:** Kiti sigue a cada agente con tres fuentes por orden de confianza: **hooks** (la fuente del semáforo), **escaneo de procesos** (vida y PID) y **archivos de sesión** (solo metadatos, y solo como apoyo). Claude Code usa el hook `http` del ADR 0004, siempre que pase el experimento E1. Codex y Gemini usan el forwarder `kiti-hook` como hook `command`. Todos los eventos se normalizan a 8 eventos de Kiti. Cada parser está versionado, se prueba con fixtures reales anonimizados y nunca guarda contenido.

## Contexto

AGENTS.md y el ADR 0001 dan por hechos algunos datos de los CLIs que había que confirmar. Este ADR fija solo el **contrato de integración**: qué escucha Kiti, qué configura y cómo distingue los procesos. El código interno, el IPC y la UI quedan fuera.

Versiones revisadas: Claude Code 2.1.310 (instalado aquí), Codex 0.164.0 y Gemini CLI 0.65.0 (npm y Homebrew). Etiquetas: **[Doc]** documentación oficial, **[Código]** código fuente oficial, **[Local]** comprobado en esta máquina, **[No verificado]** inferido, hay que probarlo antes de depender de ello.

## No reinventar: proyectos existentes

| Proyecto | Licencia | Qué aprovechar |
|---|---|---|
| [Pixel Agents](https://github.com/pixel-agents-hq/pixel-agents) (antes `pablodelucca/pixel-agents`, ~9,9k ★) | MIT | La referencia más cercana: una oficina pixel art para Claude Code. Su [`claudeHookInstaller.ts`](https://github.com/pixel-agents-hq/pixel-agents/blob/main/server/src/providers/hook/claude/claudeHookInstaller.ts) resuelve justo el merge seguro de `~/.claude/settings.json`: backup único, escritura en un tmp, *rename* atómico, relectura antes del commit y reintento. También define a mano la lista mínima de eventos y no instala `UserPromptSubmit` por privacidad. **Portar su algoritmo, con atribución.** |
| [ccusage](https://github.com/ccusage/ccusage) (~19k ★) | MIT | Tiene parsers en Rust de los transcripts de Claude, Codex y Gemini (`rust/adapters/*`). No están publicados en crates.io y están orientados a contar tokens. Sirven de **referencia o código a copiar** para el tailer, no como dependencia. |
| [codex-rs, esquemas de hooks](https://github.com/openai/codex/tree/main/codex-rs/hooks/schema/generated) | Apache-2.0 | JSON Schema oficial de entrada y salida de cada hook de Codex. Sirve para generar tipos o validar fixtures en CI. |
| [Agent Sessions](https://github.com/jazzyalex/agent-sessions) (Swift) | MIT | Lee el historial de muchos CLIs. Sirve de referencia de rutas y formatos. |
| [OpenAgentPeek](https://github.com/zuoxu3310/openagentpeek) (Tauri + Rust) | MIT | Mismo stack, con estados *working/idle/waiting* para Claude y Codex. Es pequeño (1 ★) y sirve solo para leer ideas. |
| [CodexBar](https://github.com/steipete/CodexBar) | MIT | Está centrado en los límites de uso. Fuera del alcance de Kiti. |

Ningún proyecto cubre los tres CLIs con estado por hooks y en Rust. Se escribe un adaptador propio por CLI, reutilizando lo de arriba.

## 1. Claude Code

### Eventos y payload

La [referencia de hooks](https://code.claude.com/docs/en/hooks) documenta **33 eventos** [Doc]: `SessionStart`, `Setup`, `UserPromptSubmit`, `UserPromptExpansion`, `PreToolUse`, `PermissionRequest`, `PermissionDenied`, `PostToolUse`, `PostToolUseFailure`, `PostToolBatch`, `Notification`, `MessageDisplay`, `SubagentStart`, `SubagentStop`, `TaskCreated`, `TaskCompleted`, `Stop`, `StopFailure`, `TeammateIdle`, `InstructionsLoaded`, `ConfigChange`, `CwdChanged`, `DirectoryAdded`, `FileChanged`, `WorktreeCreate`, `WorktreeRemove`, `PreCompact`, `PostCompact`, `PreModelSwitch`, `PostModelSwitch`, `Elicitation`, `ElicitationResult`, `SessionEnd`.

**Campos comunes:**

- `session_id`, `transcript_path`, `cwd`, `hook_event_name`.
- `prompt_id`, desde el primer prompt.
- `permission_mode`, `effort` y `scratchpad_dir`, según el evento.
- `agent_id` y `agent_type`, si el hook se dispara dentro de un subagente.

**Campos propios de cada evento:**

| Evento | Campos |
|---|---|
| `SessionStart` | `source`: `startup`, `resume`, `clear`, `compact` o `fork`. Además, `model` (opcional) y `session_title` |
| `UserPromptSubmit` | `prompt` |
| `PreToolUse` | `tool_name`, `tool_input` y `tool_use_id` |
| `PermissionRequest` | `tool_name`, `tool_input` y `permission_suggestions`, **sin** `tool_use_id` |
| `PostToolUse` | Además: `tool_response` y `duration_ms` |
| `PostToolUseFailure` | `error` e `is_interrupt` |
| `Notification` | `message`, `title` y `notification_type` |
| `Stop` | `stop_hook_active`, `last_assistant_message`, `background_tasks` y `session_crons` |
| `StopFailure` | `error`: `rate_limit`, `overloaded`, `authentication_failed`, `billing_error`, `server_error`, … |
| `SessionEnd` | `reason`: `clear`, `resume`, `logout`, `prompt_input_exit` u `other` |

Payload real capturado en local con `claude -p` y la versión 2.1.310 (rutas recortadas) [Local]:

```json
{
  "session_id": "aeef3d7a-13ea-4240-b0f3-ded249409206",
  "transcript_path": "/Users/USER/.claude/projects/-private-tmp-…/aeef3d7a-….jsonl",
  "cwd": "/private/tmp/…/repo",
  "prompt_id": "0663c53c-0c5a-41d8-acfe-3f2bd144ed21",
  "permission_mode": "auto",
  "effort": { "level": "high" },
  "hook_event_name": "PostToolUse",
  "tool_name": "Bash",
  "tool_input": { "command": "echo hi", "description": "Print hi" },
  "tool_response": { "stdout": "hi", "stderr": "", "interrupted": false },
  "tool_use_id": "toolu_01WKRhg2GAzbf6bdNkXVwF7v",
  "duration_ms": 494
}
```

### Hook `http`

Se configura en `~/.claude/settings.json`, que vale para todos los proyectos. También existen `.claude/settings.json`, `.claude/settings.local.json`, los managed settings y los plugins. Los hooks de distintos niveles **se suman, no se reemplazan** [Doc]. El cuerpo del POST es el mismo JSON con `Content-Type: application/json`.

**Campos:**

| Campo | Notas |
|---|---|
| `url` | Obligatorio |
| `headers` | Interpola `$VAR` solo si la variable está en `allowedEnvVars` |
| `timeout` | Por defecto **600 s**; 30 s en `UserPromptSubmit` |
| `onFailure` | `"continue"` por defecto, o `"block"`. Requiere la versión 2.1.295 o posterior |

**Si el servidor no responde** [Doc]: la conexión fallida, un estado distinto de 2xx o un cuerpo no JSON son un **error no bloqueante** y la acción continúa; si vence el timeout, el hook se cancela sin bloquear. Kiti **nunca** usa `onFailure: "block"` y responde `2xx` con cuerpo vacío: un JSON con `decision` aprobaría o denegaría acciones del usuario.

**Riesgo.** Según la documentación, los errores no bloqueantes dejan un aviso `<hook> hook error` en el transcript. **[No verificado]** si aparece en el TUI con Kiti cerrado; en `-p`, un hook `http` a un puerto cerrado no rompió nada ni emitió errores [Local]. **Experimento E1** (Hito 4; usa una sesión real de Claude, así que **requiere permiso explícito del usuario**): sesión interactiva con Kiti cerrado. Si los avisos molestan, Claude pasa al forwarder en modo `command` (siempre sale con 0) sin cambiar el contrato. Si los admins definen `allowedHttpHookUrls`, Kiti debe avisar al usuario.

```json
{ "hooks": { "PreToolUse": [ { "hooks": [ {
  "type": "http", "url": "http://127.0.0.1:PORT/v1/hooks/claude", "timeout": 2,
  "headers": { "Authorization": "Bearer TOKEN_LITERAL" } } ] } ] } }
```

Se usa el mismo bloque, sin `matcher`, para cada evento suscrito: `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `PostToolUseFailure`, `PermissionRequest`, `PermissionDenied`, `Notification`, `Stop`, `StopFailure` y `SessionEnd`.

### Instalación sin pisar los hooks del usuario

Se porta el algoritmo de Pixel Agents:

1. Leer y parsear; si falla, **abortar sin escribir**. Backup único en `settings.json.kiti.bak` (`O_EXCL`).
2. Identificar lo propio por **firma** (URL `http://127.0.0.1:<puerto>/v1/hooks/` o `command` con `/kiti-hook`), quitarlo y añadir un *matcher group* propio por evento. Lo ajeno no se toca ni se reordena.
3. Escribir a `settings.json.kiti-tmp` en el mismo directorio (mismo modo, o `0600` si es nuevo) y `fsync`.
4. **Releer el original**: si cambió (Claude Code también escribe en él), reintentar; si no, `rename` atómico. Desinstalar es lo mismo sin añadir.

Claude Code recoge los cambios con un *file watcher* [Doc]. Alternativa para más adelante: distribuir Kiti como [plugin](https://code.claude.com/docs/en/plugins/overview) con `hooks/hooks.json` (Codex y Gemini también admiten hooks en plugins o extensiones), que no edita `settings.json` pero exige que el usuario lo instale.

### Estado amarillo

El amarillo usa **`PermissionRequest`**, que se dispara en cuanto Claude va a pedir permiso. `Notification/permission_prompt` solo llega tras unos **6 s sin teclear** [Doc] y queda de respaldo, igual que `elicitation_dialog` y `elicitation_url_dialog`. `idle_prompt` llega unos 60 s después de terminar y significa gris, no amarillo.

No está documentado qué evento llega si el usuario deniega el permiso [No verificado]: el amarillo se cierra con el siguiente evento de la sesión. Además, `Stop` **no se dispara si el usuario interrumpe** con Esc [Doc], así que la oficina se quedaría en verde. El remedio es vigilar con FSEvents el campo `status` de `~/.claude/sessions/<pid>.json` (sección 4).

### Transcripts

La ruta es `~/.claude/projects/<cwd codificado>/<session>.jsonl`, por ejemplo `-home-user-work-my-repo` [Doc]. **El esquema no está documentado.**

En local hay registros de tipo `assistant`, `user`, `system`, `attachment`, `ai-title`, `permission-mode` y `api-request`. Las líneas incluyen `version`, `timestamp` y `sessionId` [Local]. Son texto plano, y la documentación advierte que pueden contener secretos [Doc](https://code.claude.com/docs/en/claude-directory).

## 2. Codex CLI

Según la [documentación de hooks](https://developers.openai.com/codex/hooks) [Doc]:

- **No hay hook `http`**: solo `command` y `mcp_tool` (`prompt` y `agent` se ignoran). **El forwarder sigue siendo necesario.**
- Los hooks están **activos por defecto** (`[features] hooks = false` los apaga).
- Configuración en `~/.codex/hooks.json` o en `[hooks]` de `~/.codex/config.toml`; las capas se suman. **Kiti usa `hooks.json`** con el mismo merge, para no tocar el TOML del usuario.
- **Hay que confiar en el hook** en `/hooks`. La confianza se guarda contra el *hash* de la definición, así que el `command` de Kiti debe ser estable entre versiones (ruta fija al binario).
- Existe `async: true` (máximo 8 en paralelo por sesión; `SessionEnd` siempre es síncrono). Timeout por defecto de 600 s; 1 s en `SessionEnd` e `Interrupt`, con máximo de 3 s.

**Eventos:** `SessionStart`, `SessionEnd`, `SubagentStart`, `SubagentStop`, `UserPromptSubmit`, `PreToolUse`, `PermissionRequest`, `PostToolUse`, `PreCompact`, `PostCompact`, `Stop` e `Interrupt`.

**Lo que falta:** no hay `Notification` ni `PostToolUseFailure`.

**Campos comunes:** `session_id` (los subagentes usan el de la sesión padre), `transcript_path` (puede ser `null`), `cwd`, `hook_event_name` y `model`. Los eventos de turno añaden `turn_id`, y la mayoría añaden `permission_mode`.

```json
{ "session_id": "thr_123", "transcript_path": "/Users/USER/.codex/sessions/2026/10/24/rollout-….jsonl",
  "cwd": "/Users/USER/proj", "hook_event_name": "PreToolUse", "model": "gpt-…",
  "turn_id": "turn_…", "permission_mode": "default",
  "tool_name": "Bash", "tool_use_id": "call_…", "tool_input": { "command": "…" } }
```

**`notify`** ([config avanzada](https://developers.openai.com/codex/config-advanced)) es el mecanismo antiguo. Solo emite `agent-turn-complete` y pasa el JSON en `argv[1]`, con `thread-id`, `turn-id`, `cwd`, `input-messages` y `last-assistant-message`. Los hooks lo hacen innecesario.

**Rollouts.** La ruta es `~/.codex/sessions/AAAA/MM/DD/rollout-AAAA-MM-DDThh-mm-ss-<uuid>.jsonl`. Cada línea tiene `{timestamp, ordinal, type, payload}`.

| Tipo de registro | Contenido |
|---|---|
| `session_meta` | `id`, `cwd`, `cli_version` y `source` |
| `turn_context` | Datos del turno |
| `event_msg` | `payload.type`: `task_started`, `item_completed` o `task_complete` (este último trae `error`) |
| `response_item` | Mensajes |

Este formato está comprobado en local [Local]. La documentación avisa de que **no es una interfaz estable**. El código ya incluye `codex migrate-rollouts`, que mueve los rollouts a un *thread store* en SQLite ([`migrate_rollouts.rs`](https://github.com/openai/codex/blob/main/codex-rs/cli/src/migrate_rollouts.rs)) [Código]. **El tailer de Codex es opcional y de bajo valor.**

## 3. Gemini CLI

Según la [documentación de hooks](https://github.com/google-gemini/gemini-cli/blob/main/docs/hooks/index.md) y su [referencia](https://github.com/google-gemini/gemini-cli/blob/main/docs/hooks/reference.md) [Doc]:

- **Solo hay `type: "command"`**, y los hooks son **síncronos**. `timeout` va en milisegundos (60 000 por defecto). El campo `name` sirve de firma (`"kiti"`).
- Configuración en `.gemini/settings.json`, `~/.gemini/settings.json`, `/etc/gemini-cli/settings.json` y extensiones. `hooksConfig.enabled` es `true` por defecto y cambiarlo exige reiniciar ([configuración](https://github.com/google-gemini/gemini-cli/blob/main/docs/reference/configuration.md)).
- Gemini toma la huella de los hooks de proyecto; **[No verificado]** si pide confirmación para los de usuario.
- stdout debe llevar **solo JSON**: el forwarder imprime `{}`.

**Eventos exactos:** `SessionStart`, `SessionEnd`, `BeforeAgent`, `AfterAgent`, `BeforeModel`, `AfterModel`, `BeforeToolSelection`, `BeforeTool`, `AfterTool`, `PreCompress` y `Notification`.

**Kiti no se suscribe a `BeforeModel`, `AfterModel` ni `BeforeToolSelection`.** Llevan el prompt completo, y `AfterModel` se dispara **por cada chunk** del stream.

**Campos base:** `session_id`, `transcript_path`, `cwd`, `hook_event_name` y `timestamp`.

| Evento | Campos propios |
|---|---|
| `BeforeTool` | `tool_name`, `tool_input` y `mcp_context` |
| `AfterTool` | Además, `tool_response` con `error` opcional |
| `BeforeAgent` | `prompt` |
| `AfterAgent` | `prompt`, `prompt_response` y `stop_hook_active` |
| `Notification` | `notification_type: "ToolPermission"`, `message` y `details` |
| `SessionStart` | `source`: `startup`, `resume` o `clear` |
| `SessionEnd` | `reason`: `exit`, `clear`, `logout`, `prompt_input_exit` u `other` |

Los hooks reciben también la variable de entorno `GEMINI_SESSION_ID`.

```json
{ "hooks": { "BeforeTool": [ { "matcher": "*", "hooks": [ {
  "name": "kiti", "type": "command", "command": "/Users/USER/.kiti/bin/kiti-hook gemini", "timeout": 1000 } ] } ] } }
```

**Chats.** La documentación de [gestión de sesiones](https://github.com/google-gemini/gemini-cli/blob/main/docs/cli/session-management.md) dice `~/.gemini/tmp/<project_hash>/chats/`. El código actual en `main` escribe en otra ruta [Código]:

- **Directorio:** `~/.gemini/tmp/<slug-del-proyecto>/chats/`. Hay un registro `projects.json` y una migración desde los antiguos directorios con hash SHA-256 ([`storage.ts`](https://github.com/google-gemini/gemini-cli/blob/main/packages/core/src/config/storage.ts)).
- **Archivo:** `session-AAAA-MM-DDThh-mm-<8 caracteres del id>.jsonl`, que es **JSONL** (antes `.json`) ([`chatRecordingService.ts`](https://github.com/google-gemini/gemini-cli/blob/main/packages/core/src/services/chatRecordingService.ts)).
- **Formato de las líneas:** metadatos `{sessionId, projectHash, …}`, mensajes con `id`, y parches `$set`, `$patch` y `$rewindTo`.

**[No verificado]** qué ruta usa exactamente la versión 0.65.0.

**Falsos positivos posibles en `~/.gemini`:** esta máquina tiene `~/.gemini/antigravity-ide/` y la app `Gemini.app` [Local]. El tailer y el scanner deben ignorarlos.

## 4. Detección por proceso en macOS

**Cómo se lee cada proceso.** `sysinfo` da:

- `exe()`, la ruta real que devuelve `proc_pidpath`.
- `cmd()` y `environ()`, que vienen de `KERN_PROCARGS2` y solo se pueden leer en procesos del mismo usuario.

**Se decide por `exe` y `argv`, nunca por el nombre del proceso.** En local, `ps` muestra Claude solo como `claude`, aunque el ejecutable real es `~/.local/share/claude/versions/2.1.310` [Local].

| CLI | Patrón positivo | Exclusiones y notas |
|---|---|---|
| Claude Code | `exe` coincide con `…/.local/share/claude/versions/<semver>` (instalador nativo) [Local], con `…/node_modules/@anthropic-ai/claude-code/bin/claude.exe` (npm, que [también es un binario nativo](https://www.npmjs.com/package/@anthropic-ai/claude-code)) o con el binario del [cask](https://formulae.brew.sh/cask/claude-code) [No verificado: ruta exacta]. **Confirmación:** existe `~/.claude/sessions/<pid>.json` | Excluir `/Applications/Claude.app/…`. Usar `kind` y `entrypoint` del archivo de sesión. Los subprocesos de Claude llevan `CLAUDECODE=1` ([env-vars](https://code.claude.com/docs/en/env-vars)) |
| Codex | `exe` con nombre `codex`: con npm, `…/vendor/<triple>/bin/codex`, lanzado por `node …/@openai/codex/bin/codex.js` ([launcher](https://github.com/openai/codex/blob/main/codex-cli/bin/codex.js)) [Código]; con el [cask](https://formulae.brew.sh/cask/codex), el binario nativo | Con npm hay **2 procesos**. El agente es el hijo nativo; el padre reenvía las señales. Excluir los subcomandos `app-server`, `mcp-server`, `login`, `completion` y `Codex.app` [No verificado: la lista completa] |
| Gemini | `exe` con nombre `node` y `argv[1]` que coincide con `(/bin/gemini|@google/gemini-cli/bundle/gemini\.js|/_npx/.+/gemini)$` | **2 procesos `node`** por sesión: un supervisor y un hijo relanzado con `--max-old-space-size` y `GEMINI_CLI_NO_RELAUNCH` ([`relaunch.ts`](https://github.com/google-gemini/gemini-cli/blob/main/packages/cli/src/utils/relaunch.ts)) [Código]. El agente es el hijo. Excluir `Gemini.app` [Local] |

Rutas de Gemini [Código; el `argv` real **No verificado**]: con npm global, el *bin* `bundle/gemini.js` enlazado en `$(npm prefix -g)/bin/gemini`; con Homebrew, `libexec/lib/node_modules/@google/gemini-cli/…` enlazado en `$HOMEBREW_PREFIX/bin/gemini` ([fórmula](https://formulae.brew.sh/formula/gemini-cli)); con npx, la caché `~/.npm/_npx/<hash>/…`. El patrón cubre las tres, pero hay que capturar el `ps` real de cada una.

**Correlación PID ↔ `session_id`**, de más a menos fiable:

1. **Claude:** `~/.claude/sessions/<pid>.json`. Su existencia está [documentada](https://code.claude.com/docs/en/claude-directory) ("un archivo pequeño por sesión en marcha"); su esquema no. En local [Local] contiene esto:

   ```json
   { "pid": 46618, "sessionId": "<uuid>", "cwd": "/Users/USER/proj", "startedAt": 1792886198259,
     "procStart": "…", "version": "2.1.310", "kind": "interactive", "entrypoint": "cli",
     "status": "idle", "updatedAt": 1792886999000 }
   ```

   `status` tomó los valores `idle` y `busy`.
2. **Codex y Gemini:** el forwarder envía solo su `getppid()` en la cabecera `X-Kiti-Ppid`. **El core** sube por los ancestros con `sysinfo` (refresco puntual de esos PIDs) hasta el primer proceso que `classify` marque como agente del mismo provider. Así la clasificación existe en un único sitio (ADR 0002). Los hooks son descendientes del CLI (pasan por `sh -c`) [Local con Claude; No verificado con Codex y Gemini].
3. **Respaldo heurístico:** el `cwd` del proceso más la hora de inicio, comparados con el primer registro del transcript o de `session_meta`, con una tolerancia de ±5 s.

## 5. Mapeo unificado

Se definen **8 eventos normalizados**. `SessionEnded` es imprescindible, porque sin él no se distingue un cierre limpio de un proceso que murió. En el código de Rust son las variantes de `Signal` (ADR 0002); la tabla de transiciones completa, con los umbrales de `Stale`, está en el ADR 0002 §3 y en `fixtures/sim/conformance/`.

| Kiti | Claude Code | Codex | Gemini | Semáforo |
|---|---|---|---|---|
| `SessionStarted` | `SessionStart` | `SessionStart` | `SessionStart` | Gris |
| `PromptSubmitted` | `UserPromptSubmit` | `UserPromptSubmit` | `BeforeAgent` | Verde |
| `ToolStarted` | `PreToolUse` | `PreToolUse` | `BeforeTool` | Verde |
| `ToolFinished` | `PostToolUse`, `PostToolUseFailure`, `PermissionDenied` | `PostToolUse` | `AfterTool` | Verde. Si no queda otra herramienta abierta y pasan más de 30 s sin eventos, Gris |
| `AwaitingInput` | `PermissionRequest`; `Notification` con `permission_prompt`, `elicitation_*` o `agent_needs_input` | `PermissionRequest` | `Notification` (`ToolPermission`) | **Amarillo** |
| `Stopped` | `Stop`; `Notification/idle_prompt`; `status: idle` en `sessions/<pid>.json` | `Stop`, `Interrupt` | `AfterAgent` | Gris |
| `Error` | `StopFailure` | Solo `task_complete.error` del rollout [No verificado] | No hay evento de error del agente [No verificado] | Rojo |
| `SessionEnded` | `SessionEnd` | `SessionEnd` | `SessionEnd` | Se retira la oficina cuando muere el proceso |
| *(scanner)* | El PID desaparece sin `SessionEnded` | Igual | Igual | Rojo (`dead`) |

**Reglas del reducer:**

- **Emparejamiento:** `PreToolUse` y `PostToolUse` se emparejan por `tool_use_id`, salvo en Gemini, que no lo tiene y se empareja por orden.
- **Errores de herramienta:** un fallo de herramienta **no** es rojo.
- **Subagentes:** los eventos con `agent_id` alimentan la sesión padre.
- **Claude y `sessions/<pid>.json`:** `status: idle` cierra el verde aunque no llegue `Stop` (interrupción con Esc) y `status: busy` lo abre. Es la segunda fuente después de los hooks.
- **Amarillo:** se cierra con el siguiente `ToolStarted`, `ToolFinished`, `PromptSubmitted` o `Stopped` de la sesión. No caduca por tiempo.

## 6. Fixtures reales para TDD

Capturar fixtures exige ejecutar los CLIs, que gastan dinero o cuota: **ningún agente lo hace sin permiso explícito del usuario**.

1. **Modo captura del forwarder.** `kiti-hook <cli> --capture <dir>` guarda el stdin crudo en `<dir>/<cli>/<versión>/<evento>-<n>.json` y no envía nada. Funciona igual para Claude.
2. **Sesiones guionizadas** en un repo desechable dentro de `/tmp`, con prompts triviales: "ejecuta `echo hi`", "edita `a.txt`", un comando que requiera permiso y una interrupción con Esc. En headless se usan `claude -p`, `codex exec` y `gemini -p`; `PermissionRequest`, `Notification` e `Interrupt` se capturan a mano en interactivo.
3. **Anonimizador**, un script del repo que corre antes de commitear: `$HOME` pasa a `/Users/USER` y los `cwd` a `/Users/USER/proj`; los UUID se sustituyen por otros deterministas; `prompt`, `tool_input.*`, `tool_response.*`, `last_assistant_message`, `prompt_response`, `message`, `llm_*` y `content` pasan a `"<redacted:N>"`, conservando tipos y claves. Un hook de pre-commit rechaza los fixtures con la ruta real del home o patrones de secretos.
4. **Contratos de terceros:** validar los fixtures de Codex contra sus [JSON Schema oficiales](https://github.com/openai/codex/tree/main/codex-rs/hooks/schema/generated). Para Claude y Gemini se usan los ejemplos de la documentación como fixtures mínimos.

## 7. Riesgos

- **Cambios de formato.** El ritmo es alto: Claude pasó de 2.1.303 a 2.1.310 en una semana [Local] y Codex ya tiene en `main` un esquema `model-request` sin documentar. Mitigación: parsers tolerantes (`serde` sin `deny_unknown_fields`); eventos desconocidos como `Unknown{name}` con métrica; cada adaptador declara el rango de versiones probado (versión desde `sessions/<pid>.json` en Claude, `session_meta.cli_version` en Codex y el `package.json` junto al script en Gemini); fixtures por versión; y un job semanal de CI que compara la documentación y los esquemas con una copia guardada.
- **Privacidad.** Los payloads y transcripts llevan código, prompts y posibles secretos. El forwarder reenvía los bytes crudos por `127.0.0.1` sin parsearlos ni guardarlos (como hace Claude con su hook `http`), y la ACL del HookServer deserializa solo una lista blanca: `session_id`, `hook_event_name`, `cwd`, `transcript_path`, `tool_name`, `tool_use_id`, `notification_type`, `source`, `reason`, el tipo de `error`, `permission_mode`, `agent_id`, `agent_type`, `turn_id`, `model`, `timestamp` y la versión, más la cabecera `X-Kiti-Ppid` del forwarder. Con Claude por HTTP el cuerpo completo llega al servidor: se descarta al deserializar y nunca va a los logs. El tailer lee solo `type`, `timestamp`, `version` y nombres de herramientas.
- **Latencia.** Codex y Gemini esperan al hook: el forwarder tiene un plazo total de 150 ms (timeout de configuración de 1 s) y siempre sale con 0, imprimiendo `{}` en Gemini y nada en el resto.
- **Fricción.** En Codex el usuario debe confiar en el hook en `/hooks`; Kiti lo detecta (proceso vivo sin eventos) y lo explica. Gemini puede requerir reiniciar.

## Cambios respecto a AGENTS.md

1. **Amarillo en Claude:** `PermissionRequest`, no `Notification` (que llega con ~6 s de retraso); `idle_prompt` es gris. La tabla de eventos de Claude se amplía con `PermissionRequest`, `PostToolUseFailure`, `StopFailure` y `SessionEnd`.
2. **Codex:** confirmado que no hay `http`. Tiene `PermissionRequest` e `Interrupt` pero no `Notification`, exige confiar en los hooks, admite `async`, y sus rollouts migran a SQLite. Con npm son 2 procesos (`node` más el binario nativo).
3. **Gemini:** corre como `node` en todas las instalaciones, con **2 procesos** por sesión. Eventos exactos en la sección 3. Los chats pasan a `~/.gemini/tmp/<slug>/chats/*.jsonl`.
4. **Claude:** el paquete npm es un binario nativo. `~/.claude/sessions/<pid>.json` da PID ↔ sesión y `busy`/`idle`. `Stop` no se dispara si el usuario interrumpe.

## Pendiente de verificar

E1 (avisos en el TUI de Claude cuando `http` no conecta); el evento de Claude al denegar un permiso; la ruta del cask de Claude; el `argv` de Gemini con npx y Homebrew y su ruta de chats en 0.65.0; si Gemini pide confirmación para los hooks de usuario; si el hash de confianza de Codex depende solo del texto del `command`; los subcomandos de Codex que no son agentes; los errores de agente en Codex y Gemini; y la cadena de ancestros del forwarder en Codex y Gemini.

## Cuándo reabrir

- Codex o Gemini añaden hooks `http`. Entonces sobra el forwarder para ese CLI.
- Claude documenta el esquema de `sessions/<pid>.json` o lo retira.
- Codex retira los rollouts JSONL.
- E1 falla. Claude pasa al forwarder.

## Revisión del arquitecto líder (2026-10-24)

Estado: **Aceptada con cambios.** Detalle en el [ADR 0006](../0006-arquitectura-consolidada/README.md).

1. **Mapeo de 8 eventos adoptado** por el dominio del ADR 0002 (`Signal::*`). El amarillo sale de `PermissionRequest`; `idle_prompt` es gris; un fallo de herramienta no es rojo; `SessionEnded` hace esperada la salida del proceso.
2. **`~/.claude/sessions/<pid>.json`** pasa a ser una fuente del dominio: el port `SessionFileSource` emite `SessionFile { pid, session_id, status }`, que es la primera regla del `Correlator` para Claude y cubre la interrupción con Esc.
3. **Ancestros en el core, no en el forwarder:** el forwarder manda `X-Kiti-Ppid` y no clasifica. Un solo `classify`, con rol `Agent` o `Launcher` por los dos procesos de Codex con npm y de Gemini.
4. **Lista blanca en un solo sitio** (la ACL). El forwarder reenvía bytes crudos.
5. **Coste:** E1 y las sesiones guionizadas para capturar fixtures ejecutan CLIs reales y **requieren permiso explícito del usuario**.
6. **Corrección:** AGENTS.md no proponía 7 eventos normalizados; solo listaba 6 hooks de Claude.
7. Lo que sigue en "Pendiente de verificar" se resuelve en los Hitos 3 a 5, no antes.

