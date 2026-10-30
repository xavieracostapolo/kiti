# ADR 0019: Agentes efímeros y tablero de tareas

- **Fecha:** 2026-10-30
- **Estado:** Aceptada (Hito 22; falta el paso 5 del plan: leer PR y CI con `gh`)
- **Decisión:** el agente que orquesta reparte el trabajo en **agentes efímeros**. Cada uno nace para una tarea con su worktree y su rama, entrega un PR y Kiti lo cierra y lo limpia. Solo quedan fijos los roles que duran.
  - Nueva herramienta MCP `kiti_spawn`: lanza un agente con un encargo escrito en un archivo, el proveedor y el modelo elegidos, en una sesión tmux de Kiti y en su propio worktree.
  - Nueva herramienta MCP `kiti_done`: el agente efímero avisa de que ha terminado (PR abierto, o fallo con el motivo) y Kiti lo cierra.
  - Un **tablero de tareas** en Kiti muestra quién hace qué, en qué rama y en qué estado.
  - Los mensajes pendientes del `Mailbox` se guardan en disco, como los alias (ADR 0017).

## Contexto

En la sesión del 2026-10-29 y 2026-10-30 un agente orquestó varios para la cuota en tiempo real (PR #23 a #26): implementación con Sonnet, documentación y merge con otro agente, y una revisión de memoria con Antigravity. Funcionó, pero el usuario dijo que se perdía: «no sé por dónde vamos». Los problemas venían de agentes que vivían más que su tarea:

- **Alias inestables:** cada reinicio de Kiti repartía los alias de nuevo. El orquestador fue `claude-8`, `claude-7`, `claude-1`… y el de documentación cambió cinco veces. Lo arregla el PR #26.
- **Mensajes perdidos:** una corrección («no borres las ramas») se perdió en un reinicio de Kiti, y el otro agente actuó con la orden anterior.
- **Agentes sueltos:** los agentes de una tarea terminada siguieron abiertos hasta que el orquestador los cerró a mano. Lanzarlos también fue a mano (`tmux new-session` con `claude --model sonnet`), y Kiti no los veía hasta reabrir la ventana.
- **Encargos largos:** `kiti_send` corta en 4000 caracteres; el encargo de los alias tuvo que ir en un archivo aparte.
- **Sin vista de conjunto:** el estado de cada tarea (rama, PR, CI, merge) solo estaba en la conversación del orquestador.

## Opciones consideradas

| Opción | A favor | En contra |
|---|---|---|
| **Agentes efímeros por tarea, lanzados por Kiti (`kiti_spawn`)** | Un agente, una tarea, una rama: nada queda suelto. Kiti sabe quién lanzó a quién y lo enseña | Kiti tiene que crear worktrees y saber cuándo cerrar |
| Seguir con agentes de larga vida y mensajes | Ya existe | Es lo que hizo perderse al usuario: roles que derivan y contexto que se acumula |
| Subagentes internos de cada CLI (p. ej. el `Agent` de Claude Code) | Sin nada nuevo en Kiti | Invisibles en el mapa, solo del mismo proveedor y sin worktree propio |
| Kiti con su propio LLM orquestador | Reparto automático | Descartado en el ADR 0017: gasta cuota sin control |

## Decisiones

### Ciclo de vida de un agente efímero

1. **Encargo:** el orquestador pasa el texto del encargo a `kiti_spawn`. Kiti lo guarda en `~/.kiti/tasks/<id>.md`.
2. **Lanzamiento:** Kiti hace `git fetch origin main` y crea el worktree y la rama `<tipo>/<slug>` desde `origin/main`. El worktree es una carpeta hermana del repo, `<repo>-<último segmento de la rama>`. Después lanza el agente en una sesión tmux nueva con el modelo pedido (por defecto, `sonnet` en Claude) y le envía un mensaje corto con su ficha de rol y la ruta del encargo.
3. **Trabajo:** el agente sale en el mapa como cualquier otra oficina y en el tablero de tareas.
4. **Entrega:** el agente llama a `kiti_done` con el PR (o `failed` con el motivo) y un resumen. Kiti avisa al orquestador con un mensaje.
5. **Cierre:** 10 segundos después de `kiti_done` (`TASK_CLOSE_DELAY`), Kiti termina la sesión, archiva la conversación del agente para que no quede 7 días como oficina apagada (ADR 0009) y borra el encargo. Borra el worktree solo si está limpio y su rama está subida; si no, lo deja y anota el motivo en la tarea.

Una tarea `Running` que lleve más de 4 horas sin `kiti_done` pasa a `Stuck` y el tablero la marca; Kiti no la cierra sola.

### `kiti_spawn`

- **Parámetros:** `role` (`implementar`, `documentar` o `revisar`), `task` (el texto del encargo), `slug`, `branch_type` (`feat`, `fix`, `docs`, `test`, `perf` o `chore`), y opcionales `provider` (`claude` por defecto, `codex` o `agy`) y `model`. El repositorio es el del orquestador que llama.
- **Coste:** lanzar un agente gasta cuota. Los rechazos devuelven un código al modelo:
  - `NEEDS_APPROVAL`: Kiti está en modo **manual** (ADR 0017). El usuario tiene que cambiar a automático o lanzar el agente él.
  - `LIMIT`: hay 3 tareas vivas (`MAX_LIVE_TASKS`) o 6 lanzamientos en la última hora (`MAX_LAUNCHES_PER_HOUR`).
  - `NO_LAUNCHER`: esta instancia de Kiti no puede lanzar agentes.
- **Permisos:** en Claude el agente arranca con `--permission-mode auto`. Nunca `bypassPermissions`.
- **Tiempos:** la llamada MCP espera hasta 25 s (`SPAWN_TIMEOUT` de `kiti-mcp`) y el HookServer corta las peticiones MCP a los 30 s.

### `kiti_done`

- **Parámetros:** `pr` (URL o número, opcional), `failed` (el motivo; si se pasa, la tarea cuenta como fallida) y `summary` (hasta 1000 caracteres).
- **Solo para agentes de una tarea abierta:** Kiti reconoce al agente por su PID. Cualquier otro recibe `NOT_A_TASK`.
- **Lo que no puede hacer:** merge ni borrar ramas. Eso queda para el usuario o para un rol fijo con su aprobación explícita.

### Aprobaciones del usuario

Las aprobaciones (merge, borrar ramas, gastar cuota) no viajan como mensajes entre agentes. En la prueba del 2026-10-30, el clasificador de permisos de Claude Code bloqueó un `gh pr merge` y un `gh pr create` de un agente efímero porque la aprobación le llegaba de otro agente, no del usuario. Es lo correcto: un mensaje no puede probar que el usuario aprobó algo. Por eso:

- El tablero de tareas tiene un botón **Aprobar** por acción sensible. Kiti lo registra con la hora y se lo comunica al agente.
- Mientras eso no exista, las acciones sensibles las hace el agente al que el usuario se lo aprobó en su propia conversación.

### Sin agentes fijos: fichas de rol

No hay agentes fijos (decisión del usuario del 2026-10-30). Cada rol es una **ficha** en [docs/agentes/](../../agentes/README.md):

- [comun.md](../../agentes/comun.md): reglas comunes;
- [implementar.md](../../agentes/implementar.md);
- [documentar.md](../../agentes/documentar.md);
- [revisar.md](../../agentes/revisar.md).

Un agente efímero recibe su ficha más un encargo concreto, y con eso sabe qué hacer sin que el orquestador se lo explique cada vez. El merge, borrar ramas y recompilar la app los hace el orquestador con la aprobación del usuario.

Por qué no un agente de publicación fijo:
- no puede hacer merge con una aprobación reenviada;
- espera sin hacer nada mientras acumula contexto (un agente fijo de esta prueba llegó a 483k tokens en Opus);
- sus aprobaciones se perdían en los reinicios de Kiti.

### Tablero de tareas

- Un panel en Kiti (⚙ en la barra superior) con una fila por tarea: rol, rama, modelo, orquestador, PR y estado (`Launching`, `Running`, `Done`, `Failed` o `Stuck`). Kiti conserva las últimas 50 tareas.
- El PR es el que el agente pasa a `kiti_done`. Leer el estado real del PR y del CI con `gh` (y los estados `mergeado` o `CI en rojo`) queda para el paso 5 del plan.
- La línea entre la oficina del orquestador y las de sus agentes no está hecha.

### Mensajes en disco

Los mensajes pendientes (`Pending`, `Queued`, `Held`) y los hilos abiertos se guardan en `mailbox.json` con el mismo port y la misma escritura atómica que `aliases.json` (PR #26). Así una corrección no se pierde si Kiti se reinicia.

## Seguridad y privacidad

- El encargo se guarda en `~/.kiti/tasks/` (carpeta `0700`, archivo `0600`) y se borra al cerrar la tarea. Nunca va a los logs (regla 6 de AGENTS.md).
- `kiti_spawn` usa el repositorio del agente que llama y crea el worktree junto a él, nunca en otra ruta. La rama solo admite `[a-z0-9/_-]`, sin `..` y de hasta 80 caracteres; el modelo, letras, números, `.`, `-` y `_`.
- Los topes de lanzamiento cortan un orquestador que entre en bucle.

## Ficha de observabilidad

| Pregunta | Señal | Umbral y verificación |
|---|---|---|
| ¿Se lanzan las tareas? | `task.spawned` (info) con proveedor, modelo y repo | Test de runtime con un `TaskLauncher` falso |
| ¿Terminan? | `task.done` (info) con resultado y duración | Toda tarea acaba en `done`, `failed` o `stuck` |
| ¿Se queda algo suelto? | `task.stuck` (warn), `task.closed` (info) | 0 sesiones de tareas cerradas que sigan vivas, en `window.__kiti.tasks()` |
| ¿Se respetan los topes? | `task.limited` y `task.rejected` (warn) | Test de política con 4 lanzamientos simultáneos y tope 3 |
| ¿Falla el lanzamiento o el aviso? | `task.launch_failed`, `task.greet_failed`, `task.notify_failed` y `task.archive_failed` (warn) | 0 en una prueba real |
| ¿Sobreviven los mensajes? | `mail.restored` (info) con el número de pendientes | Test: guardar, reiniciar el runtime y entregar |

## Plan

1. ~~Mensajes en disco~~: hecho en el PR #30 (`mailbox.json`).
2. ~~Política de tareas (topes, estados, atasco) y `kiti_spawn` / `kiti_done` en `kiti-mcp`, con el lanzador en tmux~~: hecho en el PR #35 (`domain/task.rs`, `TaskLauncher`, `src-tauri/src/tasks.rs`). El PR #33 hace que `mise run app:bundle` reinstale `kiti-mcp` y `kiti-hook`.
3. ~~Worktrees: crear y limpiar con `git worktree`, con las comprobaciones de limpio y subido~~: hecho en el PR #36 (`adapters/worktree.rs`).
4. ~~Tablero de tareas en la UI, con maqueta antes (regla 5 de AGENTS.md)~~: hecho en el PR #34 (`TaskBoard.tsx`, `?tasks=1`).
5. Lectura del PR y del CI con `gh`, y la línea del orquestador a sus agentes en el mapa: **pendiente**.

## Resultados

Prueba real del 2026-10-30, con el permiso del usuario:

- **Tarea:** la 1, con Haiku, en la rama `test/camara-e2e-estable`. Entregó el PR #37.
- **Tiempos:** 155 s desde `kiti_spawn` hasta `kiti_done`. La sesión se cerró 12 s después (10 s de `TASK_CLOSE_DELAY` más el cierre).
- **Limpieza:** sesión cerrada, conversación archivada, encargo borrado y worktree borrado. El borrado era seguro porque el commit estaba subido.

### Incidente: lefthook y `GIT_DIR`

Los tests de worktrees se ejecutaron dentro de un hook de lefthook, que exporta `GIT_DIR`. Con esa variable, git ignora `-C`: un test puso `core.bare=true` en el repositorio principal.

- **Arreglo:** el código y los tests quitan las variables `GIT_*` de cada llamada a git con la lista `GIT_ENV` (`GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE` y `GIT_COMMON_DIR`). Vale también para el tablero de la oficina, que leía el repositorio equivocado (ver CHANGELOG).
- **Regla:** todo `Command` de git del repo pasa por `GIT_ENV`.

## Cuándo reabrir esta decisión

- Si los CLIs ofrecen subagentes visibles desde fuera con worktree propio, y Kiti puede mostrarlos sin lanzarlos él.
- Si el coste de lanzar agentes efímeros (arranque y contexto inicial) supera lo que ahorra no arrastrar contexto.
