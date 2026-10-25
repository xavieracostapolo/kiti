# ADR 0010: Avisos cuando un agente te necesita

- **Fecha:** 2026-10-25
- **Estado:** Aceptada
- **Decisión:** Kiti avisa con una notificación de macOS cuando un agente **pide permiso**, **termina un turno largo**, **falla** o **se cae**. Si la ventana de Kiti está al frente no notifica, porque el usuario ya lo ve en el mapa. La política es pura y está en el dominio (`AlertPolicy`). La notificación la muestra un adapter (`AlertSink`) con el plugin oficial `tauri-plugin-notification`.

## Contexto

Con varios agentes abiertos, el usuario quiere saber cuándo uno lo necesita sin vigilar el mapa ni las terminales. La barra de menú ya muestra los contadores (🟡 🔴 🟢 ⚪), pero no avisa del cambio.

## Decisiones (elegidas por el usuario)

| Tema | Elección |
|---|---|
| Qué avisa | Pide permiso (amarillo), terminó su turno, error o caída (rojo) |
| Cuándo | Solo si la ventana de Kiti no está al frente |

## Diseño

- **`domain::AlertPolicy`** (puro) recibe los `DomainEvent` del registro y devuelve `Alert { id, kind }`:
  - `StatusChanged` a `waiting` → `Permission`.
  - De trabajando a `idle` por hook o archivo de sesión, tras un turno de **15 s o más** → `Finished`. El turno empieza en el primer `working` desde el último `idle`; esperar un permiso no lo reinicia. Un `idle` por silencio (`stale`) no cuenta como terminar.
  - `StatusChanged` a `error` → `Failed`.
  - `AgentExited { expected: false }` → `Crashed`, solo si el agente llegó a trabajar (o a esperar, o a fallar) o llevaba vivo **60 s o más**. Cerrar una oficina o un `SessionEnd` no avisan.
  - **Antirrebote:** el mismo aviso para el mismo agente, como mucho uno cada 30 s.
- **`ports::AlertSink`** recibe `AlertNotice`: tipo, agente, proveedor, proyecto, `cwd` y **título** de la conversación. Nunca recibe el pedido del usuario.
- **`application::alert_text`** arma el texto. Título: «Claude pide permiso», «Claude terminó», «Claude falló» o «Claude se cerró de forma inesperada». Cuerpo: «thoth-assets · Revisar vulnerabilidades» y, en una subcarpeta, «thoth-assets/api».
- El runtime pasa por la política los eventos de reconciliación, hooks y ticks. Sin sink no hace nada: el modo simulado no lo instala, para no generar ruido.
- **`NativeAlerts`** (src-tauri) comprueba si la ventana `main` tiene el foco. Si no lo tiene, muestra la notificación con `tauri-plugin-notification`.

## Dependencia nueva

`tauri-plugin-notification` 2.7.0. Es el plugin oficial de tauri-apps (plugins-workspace), con licencia Apache-2.0 o MIT y mantenido junto a Tauri; la 3.0 aún está en alfa. Alternativa descartada: llamar a `osascript` (`display notification`), que lanza un proceso por aviso y no tiene API.

En Linux arrastra `zbus` (D-Bus), con tres advisories de "unmaintained". Se ignoran en `deny.toml` con su motivo, como ya se hacía con gtk: en macOS no se compila.

## Privacidad

La notificación muestra el nombre del proyecto y el título de la conversación (`aiTitle`). macOS la guarda en su Centro de notificaciones. **Nunca** incluye el pedido del usuario, la actividad ni salidas de herramientas.

## Ficha de observabilidad

| Pregunta | Señal | Nivel | Campos |
|---|---|---|---|
| ¿La política decidió avisar? | `alert.raised` | info | `agent_id`, `kind` |
| ¿Se mostró? | `alert.sent` | info | `agent_id`, `kind` |
| ¿Se calló porque Kiti estaba al frente? | `alert.suppressed` | debug | `agent_id`, `reason` |
| ¿Falló el sistema de notificaciones? | `alert.error` | warn | `error` |

Ningún evento lleva títulos ni pedidos.

## Resultados

- Prueba real: al terminar un turno largo de esta misma sesión llegó «Claude terminó» (`alert.sent`, kind `Finished`).
- La misma prueba sacó ruido: tres «se cerró de forma inesperada» de procesos `claude` que vivieron segundos sin trabajar (auxiliares lanzados por otra herramienta). De ahí la regla de los 60 s.

## Límites conocidos

- Hacer clic en la notificación activa Kiti, pero si la ventana estaba cerrada no la abre ni enfoca la oficina. El plugin no expone el clic en escritorio.
- Codex y Gemini avisan igual, pero sin título (no lo tienen).
