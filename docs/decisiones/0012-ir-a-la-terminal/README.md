# ADR 0012: Ir a la terminal de un agente abierto fuera de Kiti

- **Fecha:** 2026-10-25
- **Estado:** Aceptada
- **Decisión:** el panel de un agente abierto fuera de Kiti ofrece **«Ir a su terminal»**. Kiti busca qué app lo aloja subiendo por sus procesos padre hasta la primera `.app`:
  - En **Ghostty** muestra su pestaña exacta, por carpeta y título, con su AppleScript.
  - En **Terminal.app** la muestra por tty.
  - En cualquier otra app (VS Code, por ejemplo), al menos la trae al frente.

## Contexto

Las sesiones abiertas en una terminal normal (como la del propio desarrollo de Kiti) no tienen la insignia `>_`: su terminal no vive en Kiti. Hasta ahora el panel solo decía "Abierto fuera de Kiti" y el usuario tenía que buscar la pestaña a mano.

## Investigación (en local)

- Cadena real de esta sesión: `claude` → `-/bin/zsh` → `/usr/bin/login` (root) → `/Applications/Ghostty.app/Contents/MacOS/ghostty`.
- sysinfo no lee `login` porque es de root. Para la cadena de padres se usa `ps -o ppid=,comm=`, que sí lo lee.
- Ghostty 1.3.1 trae un diccionario de AppleScript (`Ghostty.sdef`): sus `terminal` tienen `id`, `name` y `working directory`, y admiten el comando `focus`. No exponen tty ni pid. Claude Code pone como título de la pestaña el de la conversación con un indicador delante («◑ Oficina virtual 2D para agentes»).
- Terminal.app expone el `tty` de cada pestaña: el emparejamiento es exacto.
- En VS Code, el ejecutable del terminal es `…/Visual Studio Code.app/Contents/Frameworks/Code Helper.app/…`. Se toma la `.app` más externa.

## Diseño

- `adapters/focus.rs` (solo macOS):
  - Funciones puras con tests: `bundle_of`, `host_app`, `pick_ghostty` y `parse_ghostty`.
  - `focus_agent(pid, cwd, title)` devuelve `Focused(app)`, `AppOnly(app)` o `Unknown`.
- **Emparejamiento en Ghostty:**
  - Si hay una sola terminal en la carpeta del agente, es esa.
  - Si hay varias, gana la que empieza por el título de la conversación.
  - Si no hay ninguna en la carpeta, se busca por título en todas.
  - **Ante la duda, ninguna:** mejor traer la app al frente que mostrar la pestaña equivocada.
- **Sin inyección:** carpeta, título, id y tty viajan a `osascript` como argumentos (`on run argv`), nunca dentro del texto del script.
- **Comando de Tauri `focus_agent_terminal(id)`:** la UI solo manda el id del agente; carpeta y título los pone el core (`RuntimeHandle::describe` ahora devuelve `AgentBrief { cwd, title }`). El resultado es el `FocusResult` del protocolo, y el panel dice qué pasó.

## Permisos

macOS puede pedir una vez permiso de **Automatización** para que Kiti controle Ghostty o Terminal. En local, desde la terminal de desarrollo, no lo pidió.

## Ficha de observabilidad

| Señal | Nivel | Campos |
|---|---|---|
| `focus.external` | info | `agent_id`, `result` (`Focused`/`AppOnly` con el nombre de la app, `Unknown` o `NotFound`) |

Sin títulos ni carpetas en el log.

## Límites conocidos

- iTerm2, Warp y otras terminales solo se traen al frente, sin elegir la pestaña.
- Un agente dentro de un tmux propio (no el de Kiti) o por ssh da `Unknown`.
- Si en Ghostty hay varias pestañas en la misma carpeta y el agente aún no tiene título, solo se trae Ghostty al frente.
