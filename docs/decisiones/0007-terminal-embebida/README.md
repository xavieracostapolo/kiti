# ADR 0007: Terminal embebida y lanzar agentes desde Kiti

- **Fecha:** 2026-10-24
- **Estado:** Aceptada
- **Decisión:** Kiti lanza agentes en un PTY propio (`portable-pty` en Rust) y muestra su terminal con xterm.js (`@xterm/xterm` + `@xterm/addon-fit`). Solo se pueden incrustar los agentes lanzados desde Kiti; los abiertos en otra terminal muestran un aviso con su nombre.

## Contexto

El usuario quiere hacer clic en una oficina y ver la terminal del agente para interactuar con él. Sus sesiones actuales corren en Ghostty 1.3.1 (verificado: cadena `claude > zsh > login > ghostty`, una TTY por sesión).

**No se puede incrustar la terminal de un proceso que ya corre en otra app:** su TTY pertenece a Ghostty. Se evaluaron tres opciones y el usuario eligió la tercera:

1. Enfocar la pestaña de Ghostty por AppleScript (`focus`, `select tab`). Ghostty identifica cada terminal por `working directory` y `name`, no por PID ni TTY.
2. Lo anterior más escribirle con `input text` desde Kiti.
3. **Terminal embebida para agentes lanzados desde Kiti.**

Las opciones 1 y 2 quedan como mejora posible para los agentes externos.

## Decisión

**Backend (`kiti-core`, feature `pty`):** `PtyManager` con `portable-pty`.

- `spawn` arranca el CLI en un PTY con `TERM=xterm-256color`.
- Un hilo lector por sesión guarda un **scrollback circular de 512 KiB** y reenvía la salida al suscriptor actual.
- Al reabrir la ventana, `attach` devuelve el scrollback y sigue el stream sin perder bytes (la copia y el cambio de suscriptor van bajo el mismo lock).
- `write`, `resize`, `list` y `kill`.

**Seguridad:**

- La UI **nunca** envía un comando: `launch_agent(provider)` solo acepta `claude`, `codex` o `gemini`, resueltos en el `PATH`.
- La carpeta se elige con el **selector nativo desde Rust**, no desde JS.
- Los comandos `terminal_*` solo aceptan ids de PTYs creados por Kiti.

**UI:**

- `Transport.terminals` es opcional (en el simulador no existe).
- La salida llega por un `Channel` en binario.
- El panel de terminal se abre al hacer clic en una oficina lanzada desde Kiti.
- Las oficinas externas muestran un aviso con el nombre.
- Un tooltip con el nombre al pasar el ratón por cualquier oficina.

**El agente lanzado es un proceso real:** el escáner lo detecta y los hooks lo colorean como cualquier otro. La UI une oficina y terminal por PID (`list_terminals`).

## CSP

xterm.js (y otros componentes) inyectan estilos en tiempo de ejecución. Un E2E sirve el build de producción con la CSP exacta de `tauri.conf.json` y detectó que `style-src 'self'` los bloqueaba: la terminal se habría visto rota solo en la app instalada.

**Decisión:** `style-src 'self' 'unsafe-inline'` en producción. `script-src` sigue siendo `'self'`, así que un XSS no puede ejecutar código; como mucho, alterar estilos. El E2E `terminal.spec.ts` falla ante cualquier violación de CSP.

## Dependencias

| Paquete | Versión | Mantenimiento | Por qué |
|---|---|---|---|
| `portable-pty` | 0.9.0 | Último release 2025-02-11, pero el código se mantiene en el repo de WezTerm (commits en `pty/` el 2026-09-12) | Multiplataforma (ConPTY en Windows), 23 M de descargas. **Excepción** a la regla de 12 meses: se aísla detrás de `PtyManager`. Alternativa descartada: `pty-process` 0.5.4 (al día, pero solo Unix) |
| `@xterm/xterm` | 6.0.0 | 2026-10 | Ya previsto en los ADRs 0003 y 0006 |
| `@xterm/addon-fit` | 0.11.0 | 2026-10 | Ajusta columnas y filas al panel |

## Ficha de observabilidad

| Pregunta | Señal | Umbral |
|---|---|---|
| ¿Se pierde salida al reabrir? | Test de `attach` con scrollback + stream | 0 bytes perdidos |
| ¿Cuánto ocupa un agente lanzado? | Scrollback por sesión | ≤ 512 KiB |
| ¿Se lanza algo que no sea un CLI permitido? | `agent.launch` con `provider` y ruta resuelta | Solo `claude`, `codex` y `gemini` |

## Resultados (Hito 8)

- **`PtyManager`:** 5 tests contra `sh` y `cat` (salida en el scrollback, eco de la entrada, reconexión sin perder ni duplicar líneas, scrollback acotado, `kill`), estables en tres ejecuciones. `attach` envía el scrollback por el mismo sink y bajo el mismo lock que el stream, así que el orden está garantizado.
- **Comandos de Tauri:** `launch_agent` (selector nativo de carpeta + `exec <cli>` en la shell de login del usuario, para que herede su `PATH`), `list_terminals`, `attach_terminal` (bytes en crudo por `Channel`), `terminal_input` y `terminal_resize`, todos declarados en el ACL.
- **UI:**
  - Botón "+ Nuevo agente" con menú de CLIs.
  - Al hacer clic en una oficina con terminal, se abre su panel (xterm.js + fit).
  - Las oficinas externas muestran el aviso "abierto fuera de Kiti" en el panel lateral.
  - Tooltip con nombre, proveedor y estado al pasar el ratón.
- **Pruebas en el navegador:** `?terminals=1` añade al simulador una terminal falsa que repite lo que se escribe. Los E2E abren la terminal con un clic, escriben y comprueban el eco, también con el build de producción y la CSP real.
- **Pendiente manual:** lanzar un agente real desde la app (requiere elegir carpeta en el selector nativo).

## Ajustes tras la primera prueba del usuario (2026-10-25, Hito 9)

- **Fallo:** una sesión lanzada desde Kiti no tenía oficina. El escáner leía el `exe` una sola vez por proceso, y el lanzamiento es `zsh -lic 'exec claude'`: el PID se mantiene y el ejecutable cambia. Ahora el `exe` se relee en cada escaneo (6,59 → 6,76 ms), con un test que hace `exec` de un `claude` falso tras el primer escaneo.
- **Oficina primero:** al lanzar un agente ya no se abre su terminal; la cámara enfoca su oficina en cuanto aparece y la selecciona.
- **Clic del usuario** en una oficina con terminal la abre; en una sin terminal, cierra la abierta y muestra el panel con el aviso.
- **Con la terminal abierta**, todas las oficinas se reencuadran en el espacio libre de la izquierda (`fitInto` con márgenes): los semáforos siguen a la vista y se cambia de terminal con un clic. La cabecera de la terminal muestra el estado del agente y el panel de detalle se oculta para no tapar oficinas.
- **Insignia `>_`** junto al semáforo de las oficinas con terminal de Kiti.
- **Límite conocido:** el PTY pertenece a Kiti. Si Kiti se cierra del todo ("Salir"), las sesiones lanzadas desde ella terminan. Cerrar la ventana no les afecta.

## Cuándo reabrir

- Si `portable-pty` no publica un release antes de dar soporte a Windows, o aparece un advisory: se cambia por `pty-process` (Unix) más ConPTY directo.
- Si el usuario quiere interactuar con los agentes de Ghostty: opciones 1 y 2.
