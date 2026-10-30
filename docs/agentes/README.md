# Fichas de rol para agentes efímeros

En Kiti no hay agentes fijos (ADR 0019). El agente que orquesta lanza un agente efímero por tarea, con su worktree y su rama, y lo cierra cuando entrega. Para que cada uno sepa qué hacer sin que se lo expliquen cada vez, el encargo tiene dos partes:

1. **La ficha de su rol**, que está aquí y vale para cualquier tarea de ese tipo.
2. **El encargo concreto**, en un archivo que escribe el orquestador. Lleva el contexto, el diseño, los archivos, los tests y los commits de esa tarea.

## Fichas

| Ficha | Para qué | Modelo recomendado |
|---|---|---|
| [comun.md](comun.md) | Reglas que se aplican a todos los roles. Cada ficha empieza por leerla | — |
| [implementar.md](implementar.md) | Código con TDD: un arreglo, una función o un hito | Sonnet |
| [documentar.md](documentar.md) | ADR, AGENTS.md, CHANGELOG, README, web y notas de release | Sonnet o Haiku |
| [revisar.md](revisar.md) | Revisar un PR o un commit: corrección, rendimiento y memoria. No cambia nada | Sonnet |

## Cómo lanza el orquestador un agente efímero

Con `kiti_spawn` (ADR 0019):

1. Llama a `kiti_spawn` con estos parámetros:
   - `role`: `implementar`, `documentar` o `revisar`;
   - `task`: el encargo completo. Que se entienda solo: el agente nuevo no ve tu conversación;
   - `slug`: nombre corto de la rama, en minúsculas y con guiones;
   - `branch_type`: `feat`, `fix`, `docs`, `test`, `perf` o `chore`;
   - `provider` (opcional): `claude` (por defecto), `codex` o `agy`;
   - `model` (opcional): por defecto, `sonnet` en Claude.
2. Kiti guarda el encargo en `~/.kiti/tasks/<id>.md`, crea el worktree y la rama desde `origin/main`, abre la sesión tmux y envía al agente su ficha de rol y la ruta del encargo. No hace falta `kiti_send`.
3. Sigue la tarea en el tablero (⚙ en la barra superior). Si Kiti rechaza el lanzamiento, el motivo trae un código: `NEEDS_APPROVAL` (modo manual), `LIMIT` (3 tareas vivas o 6 lanzamientos por hora) o `NO_LAUNCHER`.
4. Cuando el agente llama a `kiti_done`, te llega un mensaje con el PR o el motivo del fallo. Unos 10 s después Kiti cierra su sesión, archiva su conversación, borra el encargo y borra el worktree si está limpio y su rama está subida.
5. Revisa su trabajo, pide al usuario el visto bueno para el merge y haz el merge. Borrar la rama también es tuyo.

### Alternativa manual

Si Kiti no puede lanzar la tarea (sin tmux o en modo manual), crea el worktree con `git worktree add ../<repo>-<tarea> -b <tipo>/<slug> origin/main`, escribe el encargo en un archivo y lanza el agente con `tmux -L kiti new-session -d -s kiti-<n> -c <worktree> 'claude --model sonnet --permission-mode auto'`. Después envíale con `kiti_send` un mensaje corto con su ficha y la ruta del encargo (`kiti_send` corta en 4000 caracteres). Al cerrar, busca su sesión por carpeta y proceso (`tmux -L kiti list-panes -a -F '#{session_name} #{pane_pid} #{session_path}'`), nunca por el número del alias, y archiva su conversación para que no quede 7 días como oficina apagada (ADR 0009).

Lo que **nunca** se delega en un agente efímero: el merge, borrar ramas y recompilar o reiniciar la app. El clasificador de permisos solo acepta esas acciones cuando el usuario las aprueba en la conversación de quien las ejecuta. Una aprobación que llega reenviada por otro agente no vale (ADR 0019, «Aprobaciones del usuario»).
