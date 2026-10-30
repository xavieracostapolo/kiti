# ADR 0008: Sesiones persistentes con tmux

- **Fecha:** 2026-10-25
- **Estado:** Aceptada
- **Decisión:** los agentes lanzados desde Kiti viven en sesiones de **tmux** (socket propio `-L kiti`, sesiones `kiti-N`). Kiti es solo un cliente: "Salir de Kiti" ya no termina las sesiones, y al reabrir las recupera. Además, "Nuevo agente" ofrece reutilizar la sesión de Kiti de esa carpeta o continuar la última conversación de Claude, para no crear sesiones de más.

## Contexto

Con el ADR 0007, el PTY de cada agente pertenece a Kiti: si Kiti sale, el agente muere. El usuario quiere poder retomar sus sesiones y no acumular sesiones nuevas. Se valoraron tres opciones y el usuario eligió la primera:

1. **tmux.** Un multiplexor estándar mantiene vivo el proceso; Kiti se conecta como cliente.
2. Un servicio propio de Kiti en segundo plano (launchd), separado de la ventana. Sin dependencias externas, pero es un cambio grande de arquitectura.
3. Solo retomar la conversación con `claude --resume`, sin mantener vivo el proceso.

## Decisión

- **tmux 3.8** (Homebrew). Socket propio `tmux -L kiti`, aislado del tmux personal del usuario, con un archivo de configuración de Kiti:
  - `status off`: la barra de tmux no aparece dentro de Kiti.
  - `history-limit 50000`, `escape-time 0`, `mouse on` y `default-terminal xterm-256color`.
- **Lanzar:** `tmux new-session -d -s kiti-N -c <carpeta> "<shell> -lic 'exec <cli>'"`.
  - El CLI se guarda en la opción de sesión `@kiti_provider`.
  - El PID del agente es el `pane_pid` (por el `exec`), y el escáner lo detecta como cualquier otro.
- **Ver la terminal:** `PtyManager` lanza `tmux attach-session -t kiti-N` como cliente. Al cerrar la ventana o salir de Kiti, solo muere el cliente.
- **Reabrir Kiti:** `list_terminals` lee `tmux list-panes -a` con el formato `#{session_name} #{pane_pid} #{pane_start_path} #{@kiti_provider}`.
- **Desde otra terminal:** `tmux -L kiti attach -t kiti-N`.
- **Sesiones creadas fuera de Kiti** (`tmux -L kiti new-session …` con Kiti abierto): `list_terminals` solo se pedía al abrir la ventana, así que su oficina salía sin terminal. Ahora la UI vuelve a pedir la lista cuando aparece un agente cuyo PID no tiene terminal conocida. La decisión es pura (`ui/src/state/terminalRefresh.ts`, `decideTerminalRefresh`): cada PID se pide una sola vez, porque un agente de Ghostty nunca tendrá terminal en Kiti, y como mucho una petición cada 5 s. No hay sondeo continuo.
- **Sin tmux:** se mantiene el PTY directo del ADR 0007 y la sesión termina al salir.
- **Evitar duplicados:** si la carpeta elegida ya tiene una sesión `kiti-*`, un diálogo nativo ofrece abrirla o crear otra.
- **Continuar conversación (Claude):** si `~/.claude/projects/<carpeta codificada>/` tiene conversaciones, el diálogo ofrece `claude --continue` o empezar una nueva. La codificación sustituye por `-` todo carácter no alfanumérico, como se observó en local.

## Consecuencias

- Las sesiones sobreviven a Kiti, pero no a un reinicio del Mac (tmux tampoco).
- Hay una dependencia externa; si falta, todo sigue funcionando sin persistencia.
- Cerrar una oficina mata el proceso del agente, y la sesión tmux desaparece con él (`remain-on-exit off`).

## Resultados (Hito 10)

- **Adapter `Tmux`:** 4 tests contra tmux real en sockets aislados (crear y listar con PID, carpeta y proveedor; nombres únicos; el cliente por PTY escribe y lee, y **la sesión sobrevive al cierre del cliente**; la sesión desaparece cuando termina el agente). Los tests borran sus servidores y sus archivos de socket.
- **Prueba de extremo a extremo con la app:** una sesión `kiti-1` con un `claude` falso apareció como oficina, siguió viva con el mismo PID tras salir de Kiti y, al reabrir Kiti, `list_terminals` la devolvió con su proveedor y carpeta.
- **Lanzar:** "Nuevo agente" pregunta si reutilizar una sesión de Kiti en la misma carpeta y, en Claude, si continuar la última conversación (`claude --continue`). Cada lanzamiento registra en el log la orden para entrar desde otra terminal (`tmux -L kiti attach -t kiti-N`).
- **Sin tmux** (`terminals.ready persistent=false`), Kiti usa el PTY directo del ADR 0007.
- **Pendiente manual:** los dos diálogos nativos y una sesión real de Claude dentro de tmux.
