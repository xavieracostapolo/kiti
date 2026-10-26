# ADR 0009: Proyectos, conversaciones y actividad

- **Fecha:** 2026-10-25
- **Estado:** Aceptada
- **Decisión:** el mapa se organiza en **zonas por proyecto** (raíz del repo git, o la carpeta si no hay git). Las conversaciones cerradas de los últimos 7 días aparecen como **oficinas apagadas** que se pueden **retomar** (`claude --resume <id>` en tmux). Cada oficina muestra el **título de la conversación**, la **actividad actual** y el **último pedido** del usuario.

## Contexto

El usuario trabaja con varias sesiones por proyecto (por ejemplo, en un mismo repositorio, una de vulnerabilidades y otra de migración de datos), las cierra y quiere retomarlas. También quiere ver agrupado en qué trabaja en cada proyecto y qué hace cada agente sin abrir su terminal. El problema se planteó sin solución previa y las decisiones se tomaron con él.

**Conceptos:**

- **Proceso**: un `claude` vivo. Es una oficina encendida.
- **Conversación**: el historial en `~/.claude/projects/<carpeta>/<id>.jsonl`. Sobrevive al proceso y se puede retomar.
- **Subagente**: un ayudante que Claude lanza dentro de una conversación; no es un proceso aparte.

Cuando el contexto se llena, Claude **compacta** y sigue en la misma conversación.

**Datos observados en local** (solo la estructura, sin leer el contenido):

- El proyecto del caso de uso tiene 5 conversaciones guardadas.
- Tres de ellas tienen registros `ai-title` con `aiTitle`.
- Todas tienen registros `last-prompt` con `lastPrompt`.

## Decisiones (elegidas por el usuario)

| Tema | Elección |
|---|---|
| Agrupar | Por repositorio git; si no hay, por carpeta |
| Conversaciones cerradas | Últimos 7 días, como oficinas apagadas dentro de su zona |
| Información de cada oficina | Título (`aiTitle`), actividad actual (hooks) y último pedido (`lastPrompt` / `UserPromptSubmit`) |

## Privacidad (cambio respecto al ADR 0005 §7)

Hasta ahora Kiti solo extraía metadatos. Con este ADR también lee:

- **El último pedido del usuario**, recortado a 140 caracteres.
- **La descripción de la herramienta**: el campo `description` de `Bash`, el nombre del archivo (sin la ruta), el tipo de subagente y la tarea en curso de la lista de tareas.

Todo queda **en la máquina**: viaja solo del core a la UI por IPC, **nunca se escribe en los logs** y no sale a la red. Siguen sin leerse los argumentos completos, las salidas de las herramientas y las respuestas de Claude.

## Diseño

**Core (Rust):**

- `ingestion/claude.rs` traduce lo nuevo de los hooks a `AgentInfo { title, activity, prompt }`: la actividad sale de `PreToolUse` (por ejemplo "Editando main.rs" o "Subagente Explore: …") y el pedido, de `UserPromptSubmit`. Del transcript salen `aiTitle` y `lastPrompt`. Todo se recorta a 140 caracteres.
- `AgentRegistry::hook_info` enruta la info igual que la señal, también cuando el hook queda retenido a la espera de enlazar su sesión. Al enlazar, el agente guarda su `sessionId`.
- `domain::project_root` es puro: sube desde el `cwd` hasta el primer directorio con `.git`. El adapter `GitProjects` (port `ProjectResolver`) consulta el disco y guarda el resultado en caché. El runtime lo usa al proyectar cada agente; en el simulador cada carpeta es su proyecto (`CwdProjects`).
- `adapters/conversations.rs` (`ConversationIndex`) lista `~/.claude/projects/*/<id>.jsonl` modificados desde un corte. De cada uno lee los últimos 256 KiB y guarda el resultado por fecha y tamaño. Descarta las conversaciones sin título ni pedido.
- Protocolo: `AgentState` gana `project`, `sessionId`, `title`, `activity` y `lastPrompt`. El nuevo `ConversationInfo` describe una conversación cerrada.
- Tauri: `list_conversations` (7 días, máximo 60) y `resume_conversation(sessionId)`. Este último solo acepta ids que el índice ya listó y que tienen solo letras, dígitos y guiones. Lanza `exec claude --resume <id>` en su carpeta con el mismo camino que "Nuevo agente": tmux si está, si no PTY directo (`spawn_cli`).

**UI:**

- `domain/zones.ts` es puro. Agrupa por proyecto, ordena las zonas por nombre y las coloca en filas. Dentro de una zona cada oficina conserva su slot (`assignSlots`).
- `state/rooms.ts` calcula las oficinas apagadas: las conversaciones que no tienen un agente encendido con su `sessionId`.
- El mundo dibuja un área por zona con su nombre y su número de oficinas. Cada oficina muestra el título en lugar de la carpeta y, abajo, una franja con la actividad (o el último pedido). La ventana de la pared se estrechó para que quepan 19 caracteres de título.
- Una oficina apagada está en penumbra, sin semáforo ni avatar y con "zZ". Su tooltip y su panel (`ConversationPanel`) ofrecen **Retomar**. Retomar sigue el principio "oficina primero": la oficina del agente se enfoca y se selecciona, y la terminal se abre con un clic.
- La UI pide la lista al conectar, cada 15 s y cuando se cierra una oficina.

## Resultados del Hito 11

- **Datos reales** (conteos, sin leer contenido): 7 conversaciones en los últimos 7 días, en 5 carpetas. Incluye el proyecto del caso de uso. La primera lectura tarda 24 ms y las siguientes, 0,4 ms (caché).
- **Rendimiento:** con 50 agentes, el p95 de frame sigue en 2 ms y el delta de rAF en 17 ms (igual que antes).
- **Bug corregido de paso:** `Culler.shared.cull` usaba por defecto las transformaciones del frame anterior. Tras un salto de cámara sin animación, las oficinas quedaban ocultas. Ahora se llama con `skipUpdateTransform = false`.
- **Tests:** dominio de proyecto, enrutado de info retenida, meta de conversaciones e índice en disco (Rust); layout de zonas, salas apagadas, store, tooltip y panel (Vitest); E2E de zonas, tooltip y retomar con el simulador (`?conversations=1`). Las líneas base visuales se regeneraron con las zonas.

## Ajustes tras probar la app real

- **Permisos de los commands:** `list_conversations` y `resume_conversation` estaban registrados, pero faltaban en `build.rs` y en la capability. Tauri los rechazaba ("not allowed by ACL"). Ahora `src-tauri/tests/acl.rs` falla si un command registrado no está permitido.
- **"Cerrar oficina" no hacía nada en la app nativa** (desde el Hito 2): el botón marcaba la oficina como pendiente, pero solo el diálogo del simulador consumía esa marca. `NativeCloseOffice` pasa cada petición a `kill_agent`, que enseña el diálogo nativo.
- **Archivar** (pedido del usuario): una oficina apagada se puede archivar desde su panel, con confirmación. Su id se guarda en `hidden-conversations.json`, en la carpeta de datos de Kiti, y `list_conversations` la descarta. No se borra nada de Claude: la conversación se puede seguir retomando con `claude --resume`. No hay "desarchivar" en la UI; se hace quitando el id de ese archivo.
- **Texto que se adapta al zoom** (pedido del usuario): el título de la pared, la nota inferior y los rótulos de zona miden más o menos lo mismo en pantalla. Al acercarse, la letra se hace más pequeña en el mundo, así caben el título entero (hasta 3 líneas) y el pedido o la actividad (hasta 4 líneas). `domain/text.ts` (`fontFor`, `fitLines`) es puro y cuantiza a medio píxel para no recalcular el texto en cada frame del zoom.
- El puente de logs de la UI escribe los errores de los commands como JSON en lugar de `[object Object]`.

- **Verde desde el arranque (Hito 15):** si Kiti arranca mientras un agente ya trabaja, el `busy` de su archivo de sesión es anterior al registro del agente y se descartaba por "atrasado". Ahora un agente sin eventos todavía (`Startup`) acepta el status del archivo aunque sea más viejo: es lo único que se sabe de él.

## Ficha de observabilidad

| Evento | Nivel | Campos |
|---|---|---|
| `conversations.listed` | debug | `count`, `duration_ms` |
| `conversation.resumed` | info | `pid` |
| `conversation.hidden` | info | (sin campos) |
| `agent.launch` | info | `provider`, `cwd`, `pid`, `session`, `hint` (también al retomar) |

Ningún evento lleva títulos, pedidos ni actividad. El id de la conversación tampoco se registra.

## Límites conocidos

- Solo Claude tiene conversaciones que retomar, título y último pedido. Codex y Gemini solo muestran el semáforo y la herramienta.
- Si `aiTitle` quedó antes de los últimos 256 KiB del transcript, la oficina apagada muestra la carpeta en lugar del título.
- `claude --resume` mantiene el mismo id de conversación. Hasta que el hook `SessionStart` o el archivo de sesión enlazan el proceso (≤ 3 s), la UI quita la oficina apagada por su cuenta.
- La caché de raíces git no se invalida: un `git init` posterior se ve al reiniciar Kiti.
