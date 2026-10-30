# ADR 0019: Agentes efímeros y tablero de tareas

- **Fecha:** 2026-10-30
- **Estado:** Propuesta (pendiente del visto bueno del usuario)
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

1. **Encargo:** el orquestador escribe el encargo en `~/.kiti/tasks/<id>.md` (o pasa el texto) y llama a `kiti_spawn`.
2. **Lanzamiento:** Kiti crea el worktree `<repo>-<id>` y la rama `<tipo>/<slug>` desde `origin/main`, y lanza el agente en una sesión tmux nueva con el modelo pedido (por defecto, el más barato del proveedor que sirva para la tarea). El primer mensaje del agente es el encargo.
3. **Trabajo:** el agente sale en el mapa como una oficina hija de la de su orquestador, con la insignia de tarea.
4. **Entrega:** el agente llama a `kiti_done` con el PR o el motivo del fallo. Kiti avisa al orquestador con un mensaje.
5. **Cierre:** Kiti termina la sesión. Si el worktree está limpio y su rama está subida, lo borra; si no, lo deja y lo marca en el tablero.

Un agente efímero que lleve más de N horas sin `kiti_done` (por defecto 4) sale en ámbar en el tablero; Kiti no lo cierra solo.

### `kiti_spawn`

- **Parámetros:** proveedor (`claude`, `codex`, `agy`), modelo, repositorio, tipo de rama, encargo (texto o ruta) y si puede hacer push y abrir PR.
- **Coste:** lanzar un agente gasta cuota. En modo **manual** (ADR 0017), cada `kiti_spawn` pide la aprobación del usuario en Kiti. En modo **automático** hay un tope de agentes efímeros simultáneos (por defecto 3) y de lanzamientos por hora (por defecto 6).
- **Permisos:** el agente hereda el modo de permisos que el usuario fije para las tareas (por defecto `auto`). Nunca `bypassPermissions`.
- **Lo que no puede hacer:** merge ni borrar ramas. Eso queda para el usuario o para un rol fijo con su aprobación explícita.

### Roles fijos

Solo son fijos los roles que duran más que una tarea; por ahora, **publicación** (docs, CI y merge con el visto bueno del usuario). Todo lo demás (implementar, revisar, regenerar capturas, medir memoria) es efímero.

### Tablero de tareas

- Un panel en Kiti (y su chip en la barra superior) con una fila por tarea: agente, orquestador, rama, PR, estado de CI y estado (`en curso`, `PR abierto`, `mergeado`, `fallida`, `atascada`).
- En el mapa, la oficina del orquestador une con una línea a sus agentes efímeros.
- El estado del PR y del CI se lee con `gh` cada minuto, solo mientras haya tareas abiertas.

### Mensajes en disco

Los mensajes pendientes (`Pending`, `Queued`, `Held`) y los hilos abiertos se guardan en `mailbox.json` con el mismo port y la misma escritura atómica que `aliases.json` (PR #26). Así una corrección no se pierde si Kiti se reinicia.

## Seguridad y privacidad

- El encargo se guarda en `~/.kiti/tasks/` con permisos `0600` y se borra al cerrar la tarea. Nunca va a los logs (regla 6 de AGENTS.md).
- `kiti_spawn` solo acepta repositorios del usuario que ya estén en el mapa, y crea los worktrees junto a ellos, nunca en otra ruta.
- Los topes de lanzamiento cortan un orquestador que entre en bucle.

## Ficha de observabilidad

| Pregunta | Señal | Umbral y verificación |
|---|---|---|
| ¿Se lanzan las tareas? | `task.spawned` (info) con proveedor, modelo y repo | Test de runtime con un `TaskLauncher` falso |
| ¿Terminan? | `task.done` (info) con resultado y duración | Toda tarea acaba en `done`, `failed` o `stuck` |
| ¿Se queda algo suelto? | `task.stuck` (warn) | 0 sesiones de tareas cerradas que sigan vivas, en `window.__kiti.tasks()` |
| ¿Se respetan los topes? | `task.limited` (warn) | Test de política con 4 lanzamientos simultáneos y tope 3 |
| ¿Sobreviven los mensajes? | `mail.restored` (info) con el número de pendientes | Test: guardar, reiniciar el runtime y entregar |

## Plan

1. Mensajes en disco (pequeño; reutiliza el port de los alias).
2. `TaskPolicy` pura (topes, estados, atasco) y `kiti_spawn` / `kiti_done` en `kiti-mcp`, con el lanzador en tmux.
3. Worktrees: crear y limpiar con `git worktree`, con las comprobaciones de limpio y subido.
4. Tablero de tareas en la UI, con maqueta antes (regla 5 de AGENTS.md).
5. Lectura del PR y del CI con `gh`.

## Cuándo reabrir esta decisión

- Si los CLIs ofrecen subagentes visibles desde fuera con worktree propio, y Kiti puede mostrarlos sin lanzarlos él.
- Si el coste de lanzar agentes efímeros (arranque y contexto inicial) supera lo que ahorra no arrastrar contexto.
