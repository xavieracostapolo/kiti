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

Mientras no exista `kiti_spawn` (ADR 0019):

1. Crea el worktree y la rama: `git worktree add ../kiti-<tarea> -b <tipo>/<slug> origin/main`.
2. Escribe el encargo concreto en un archivo.
3. Lanza el agente en una sesión tmux de Kiti:

   ```bash
   tmux -L kiti -f "$HOME/Library/Application Support/com.kiti.desktop/tmux.conf" \
     new-session -d -s kiti-<n> -c <worktree> 'claude --model sonnet --permission-mode auto'
   tmux -L kiti set-option -t kiti-<n> @kiti_provider claude
   ```

4. Busca su alias con `kiti_agents` (muestra también su sesión tmux y su PID, para ligar el alias con su terminal) y envíale con `kiti_send` un mensaje corto: «Tu rol: `docs/agentes/<rol>.md`. Tu encargo: `<ruta>`. Léelos enteros y síguelos.» `kiti_send` corta en 4000 caracteres, así que el detalle va en los archivos.
5. Cuando entrega, revisa su trabajo, pide al usuario el visto bueno para el merge, haz el merge y cierra su sesión, su worktree y su rama local.
   - Para saber qué sesión tmux cerrar, búscala por carpeta y proceso, nunca por el número del alias: `claude-4` no tiene por qué vivir en `kiti-4`.
   - Lista las sesiones con `tmux -L kiti list-panes -a -F '#{session_name} #{pane_pid} #{session_path}'` y, si hay dudas, mira `~/.claude/sessions/<pid>.json`, que liga el proceso a su conversación.
   - Archiva también su conversación, o Kiti la seguirá mostrando 7 días como oficina apagada (ADR 0009). Para ello, añade su id (el nombre del `.jsonl` en `~/.claude/projects/<carpeta del worktree>/`) a `hidden-conversations.json`, en el app data dir, con Kiti cerrado. Si no, usa el botón de archivar de la oficina. Con `kiti_done` (ADR 0019) lo hará Kiti.

Lo que **nunca** se delega en un agente efímero: el merge, borrar ramas y recompilar o reiniciar la app. El clasificador de permisos solo acepta esas acciones cuando el usuario las aprueba en la conversación de quien las ejecuta. Una aprobación que llega reenviada por otro agente no vale (ADR 0019, «Aprobaciones del usuario»).
