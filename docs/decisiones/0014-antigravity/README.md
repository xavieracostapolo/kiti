# ADR 0014: Antigravity CLI como proveedor

- **Fecha:** 2026-10-25
- **Estado:** Aceptada
- **Decisión:** Kiti soporta **Antigravity CLI** (`agy`) como cuarto proveedor (`antigravity`) con lo mismo que Claude:
  - detección y semáforo por hooks;
  - amarillo leyendo su base de datos de pasos;
  - lanzarlo desde Kiti;
  - título, actividad y último pedido;
  - conversaciones cerradas que se retoman con `agy --conversation <id>`.

  Gemini CLI se mantiene como proveedor (para quien lo use con API key de pago), pero deja de ofrecerse en «Nuevo agente».

## Contexto

El usuario pidió soportar Gemini. Al instalarlo, Homebrew marcó `gemini-cli` como obsoleto. Google sustituyó Gemini CLI por Antigravity CLI el 18-06-2026 para las cuentas gratuitas y de Google One; Gemini CLI solo sigue funcionando con API keys de pago o Enterprise. El usuario eligió **Antigravity CLI** (cask `antigravity-cli`, versión 1.4.7) e inició sesión con su cuenta.

## Lo observado con el CLI real (capturas redactadas, sin texto)

- **Proceso:** uno solo, con `argv[0] = agy`. Su ejecutable real es `/opt/homebrew/Caskroom/antigravity-cli/<versión>/antigravity` y no tiene procesos hijos. Antigravity IDE es otra app (`/Applications/Antigravity IDE.app`) y se ignora.
- **Hooks** en `~/.gemini/config/hooks.json`. Son comandos que reciben el JSON por stdin, que **no dice qué evento es**; el evento se pasa como argumento.
- **Turno con una herramienta que pide permiso:**

  | Momento | Evento |
  |---|---|
  | Empieza la sesión | `SessionStart` |
  | Antes de cada llamada al modelo | `PreInvocation` (`invocationNum` 0, 1…) |
  | Antes de la herramienta | `PreToolUse` (`toolCall.name = run_command`) |
  | Esperando permiso del usuario | **ningún evento** |
  | Tras la herramienta | `PostToolUse` (`error` vacío si fue bien) |
  | Tras cada llamada al modelo | `PostInvocation` |
  | Fin del turno | `Stop` (`terminationReason`, `fullyIdle: true`) |

- **Campos comunes:** `conversationId`, `workspacePaths` (la carpeta del proyecto), `transcriptPath`, `artifactDirectoryPath`, `modelName` y `lastUserInput` (el último pedido; no aparece en la documentación).
- **Respuesta del hook:** `PreToolUse` exige `decision`. `"allow"` delega en la política normal, así que Kiti no aprueba ni bloquea nada. `Stop` también exige `decision`; cualquier valor salvo `"continue"` deja que el turno termine.
- **Permiso:** mientras espera tu aprobación, su paso queda con `status = 9` en `~/.gemini/antigravity-cli/conversations/<id>.db` (tabla `steps`) y pasa a `3` al aprobarlo. `sqlite3 -readonly` lo lee directamente en unos 3 ms.
- **Título y conversaciones:** `~/.gemini/antigravity-cli/conversation_summaries.db` guarda por conversación `title`, `workspace_uris`, `last_modified_time` y `status`.

## Diseño

- **Proveedor `antigravity`** en dominio y protocolo. Insignia `A`, color violeta y alfombra turquesa, compartida con Gemini (los dos son de Google).
- **Detección** (`classify`): `argv[0]` con basename `agy`, o ejecutable bajo `…/antigravity-cli/…/antigravity`. Los subcomandos que no son agentes no cuentan: `models`, `mcp`, `plugin`, `remote-control`, `mic-serve`, `update`, `install`, `help`, `changelog`, `agent(s)`.
- **Reenviador:** `kiti-hook antigravity <Evento>` envía el cuerpo tal cual, más `X-Kiti-Event` y `X-Kiti-Ppid`. Pase lo que pase con Kiti, imprime siempre la respuesta neutra: `{"decision":"allow"}` en `PreToolUse` y en `Stop`, y `{}` en el resto.
- **ACL (`ingestion/antigravity.rs`):**

  | Evento | Señal |
  |---|---|
  | `SessionStart` | `SessionStart` |
  | `PreInvocation` con `invocationNum = 0` | `PromptSubmit` |
  | `PreToolUse` | `ToolStart` (clave: conversación + `stepIdx`) |
  | `PostToolUse` | `ToolEnd` (falla si `error` no está vacío) |
  | `Stop` con `fullyIdle` | `Stop` (o `StopFailure` si trae `error`) |

  La sesión es `conversationId` y la carpeta, `workspacePaths[0]`. La actividad sale de `toolSummary`, o del nombre del archivo (`Path`). El pedido sale de `lastUserInput`. Todo se recorta a 140 caracteres, igual que en Claude (ADR 0009).
- **Amarillo y título:** en cada escaneo, `AntigravityWatcher` lee `conversation_summaries.db` y, solo para las conversaciones en curso, cuenta los pasos con `status = 9`. Si hay alguno, manda `AwaitingInput`; además actualiza el título.
- **Instalador:** `mise run hooks:install-antigravity` añade la entrada `kiti` a `~/.gemini/config/hooks.json`, respeta las que ya haya y hace copia de seguridad. `hooks:uninstall` la quita.
- **Lanzar y retomar:**
  - «Nuevo agente → Antigravity CLI» ejecuta `agy` (o `agy --continue`) en tmux.
  - Las conversaciones cerradas de los últimos 7 días salen de `conversation_summaries.db` y se retoman con `agy --conversation <id>` en su carpeta.

## Privacidad

Lo mismo que en el ADR 0009: el último pedido y la actividad, recortados, solo viajan del core a la UI y nunca se escriben en los logs. Kiti no lee el contenido de los pasos, salvo su `status`.

## Ficha de observabilidad

| Pregunta | Señal | Umbral / verificación |
|---|---|---|
| ¿Llegan los hooks de Antigravity? | `hook.received` con `provider=antigravity` | Un turno real produce `SessionStart`, `PreToolUse`… en el log de depuración |
| ¿Se detecta el amarillo? | `status.changed` a `Waiting` con `reason=Hook` | Un comando que pide permiso pone la oficina en amarillo en menos de 3 s |
| ¿Cuesta leer las bases de datos? | `antigravity.poll` (debug) con `duration_ms`, `conversations` y `waiting` | Menos de 10 ms por escaneo con 3 conversaciones en curso |

## Resultados (prueba real con `agy` 1.4.7)

- **Detección:** sysinfo da el symlink con el que se lanzó (`/opt/homebrew/bin/agy`), no el binario del cask: se aceptan los dos nombres. La sesión que el usuario abrió en su terminal apareció como oficina de Antigravity en la zona `kiti`.
- **Semáforo completo con un comando que pide permiso:**
  1. Verde por hook al empezar el turno.
  2. **Amarillo unos 5 s después**, por la lectura de la base de datos, con el aviso «Antigravity pide permiso».
  3. Verde al aprobarlo y gris en el `Stop`, con el aviso «terminó».
- **Título:** el de `conversation_summaries.db` («Ejecutar Comando Date Y Fecha»). Las conversaciones cerradas aparecen como oficinas apagadas con su título y su último pedido.
- **Coste:** leer las conversaciones de los últimos 7 días, con su carpeta, título y último pedido, tardó 12,6 ms. Solo se lee si hay algún `agy` vivo.
- **Base de datos sin `-wal`** (agy cerrado): no abre en solo lectura (error 14, porque necesitaría crear el `-shm`). Por eso se usa `immutable=1` cuando no hay `-wal`.
- **Respuesta del hook:** `{"decision":"allow"}` no cambió nada; `agy` pidió permiso como siempre.

## Bug de Claude encontrado de paso

Claude Code ejecuta sus búsquedas (`ugrep`, `rg`) con su propio binario (`~/.local/share/claude/versions/<semver>`), así que tienen el mismo `exe` pero otro `argv[0]`. Kiti las contaba como agentes. Vivían unos segundos y daban falsas «caídas»: esos eran los avisos de ruido del ADR 0010. Ahora `classify` exige que el `argv[0]` sea `claude`; sin argv legible, se sigue tratando como agente.

## Límites conocidos

- El amarillo llega en el siguiente escaneo (cada 3 s), no al instante como con un hook.
- `status = 9` está observado en la 1.4.7, no documentado. Si una versión lo cambia, el amarillo deja de verse, pero el resto del semáforo sigue funcionando.
- Los hooks se instalan a mano (`mise run hooks:install-antigravity`), como los de Codex y Gemini.

