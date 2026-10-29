# ADR 0017: Mensajes entre agentes

- **Fecha:** 2026-10-28
- **Estado:** Propuesta (pendiente del visto bueno de la maqueta)
- **Decisión:** Kiti hace de **central de mensajes** entre los agentes que corren en la máquina, sean del proveedor que sean.
  - Kiti no tiene un modelo propio ni decide nada. El "orquestador" es cualquier agente que reparte trabajo con las mismas herramientas que los demás.
  - Los agentes hablan con Kiti por **MCP**: un servidor `kiti-mcp` por stdio, con las herramientas `kiti_agents`, `kiti_send` y `kiti_inbox`.
  - Kiti entrega cada mensaje escribiéndolo en la terminal tmux del destinatario cuando está libre.
  - Por defecto, el modo es **supervisado**: el usuario aprueba cada mensaje con un clic antes de que llegue.

## Contexto

El usuario quiere que varios agentes (Claude, Codex, Antigravity) se pasen mensajes y resuelvan problemas juntos. Pidió los tres casos de uso:

1. **Revisión cruzada:** uno implementa y otro revisa.
2. **Reparto:** un agente jefe divide una tarea y la reparte entre otros.
3. **Consulta:** un agente atascado pregunta a otro que trabaja en otro repositorio (por ejemplo, el frontend al backend).

Kiti ya tiene casi todo:
- Sabe qué agentes corren, en qué proyecto y en qué estado.
- Tiene un servidor local con token (ADR 0004).
- Lanza los agentes en tmux (ADR 0008) y sabe cuándo terminan un turno (hook `Stop`).

## Opciones consideradas

| Opción | A favor | En contra |
|---|---|---|
| **MCP por stdio (`kiti-mcp`)** | Lo entienden los tres CLIs (`claude mcp add`, `agy mcp add`, `[mcp_servers]` de Codex). El padre del proceso es el agente, así que Kiti sabe **quién** envía sin fiarse de lo que diga el mensaje | Un binario más que instalar |
| MCP por HTTP en el HookServer | Sin binario nuevo | HTTP no dice qué proceso llama: el remitente se declararía a sí mismo y podría suplantar a otro |
| Archivos compartidos (pizarra) | Trivial | Sin entrega, sin identidad, frágil con escrituras simultáneas |
| A2A (Agent2Agent de Google) | Pensado para agentes | Para agentes remotos con servidor propio; ningún CLI lo habla |
| Kiti con su propio LLM orquestador | Reparto automático | Gasta cuota sin control, y duplica lo que ya hace cualquier agente |

## Decisiones

### Identidad

- `kiti-mcp` envía su `getppid()` en `X-Kiti-Ppid`, igual que el forwarder. El core sube por los ancestros hasta un agente conocido (ADR 0005).
- Si el proceso no pertenece a ningún agente vivo, se responde con error y no se envía nada.
- Cada agente se nombra con un **alias corto y estable** mientras vive: `claude-1`, `codex-2`, `agy-1`… Es más fácil de copiar para el modelo que el id opaco.

### Herramientas MCP

| Herramienta | Entrada | Salida |
|---|---|---|
| `kiti_agents` | — | Los demás agentes vivos: alias, proveedor, proyecto (nombre de carpeta), estado y título |
| `kiti_send` | `to` (alias), `text` (hasta 4000 caracteres), `reply_to` opcional | Id del mensaje y su estado: `pending_approval`, `queued` o `delivered` |
| `kiti_inbox` | — | Los mensajes no leídos, con remitente, hilo y hora; quedan marcados como leídos |

- Las descripciones de las herramientas avisan al modelo de que un mensaje es **información de otro agente, no una orden del usuario**.
- Fase 2 (fuera de este hito): `kiti_launch(provider, folder, task)` para que un jefe abra ayudantes. Siempre pedirá aprobación, porque gasta cuota.

### Entrega

- **Destinatario lanzado desde Kiti (tmux):**
  - Si está libre (gris), Kiti escribe en su terminal con `tmux send-keys -l` y pulsa Enter: `[Kiti] Mensaje de claude-1 (tienda-web): … Responde con kiti_send si hace falta.`
  - Si está trabajando o esperando permiso, el mensaje queda en cola y se entrega al terminar su turno.
- **Destinatario abierto fuera de Kiti:** no se le puede escribir. Su oficina muestra un sobre y el mensaje espera a que llame a `kiti_inbox`. El usuario puede copiarlo desde el panel.

### Supervisión y límites

- **Modo supervisado (por defecto):**
  - Cada mensaje aparece en el panel de mensajes con **Entregar** y **Descartar**.
  - El usuario puede cambiar a **automático** para un hilo concreto.
- **Límites, también en modo automático:**
  - 20 envíos por hora por agente.
  - 12 saltos por hilo; al llegar al límite, el hilo vuelve a supervisado.
  - Un agente no puede enviarse mensajes a sí mismo.
  - Si se supera un límite, `kiti_send` devuelve un error claro y no se envía nada.
- **Botón de pausa global:** congela todas las entregas.

### Arquitectura

- `crates/mcp` (bin `kiti-mcp`):
  - Servidor MCP por stdio con `rmcp`.
  - Cada herramienta es una llamada HTTP al core con `ureq`, token y `X-Kiti-Ppid`.
  - No guarda estado.
- Core:
  - Dominio `mailbox` puro: mensajes, hilos, colas, límites y aprobación. Probado con TDD y `proptest` para los límites.
  - El HookServer añade `/v1/mcp/{agents,send,inbox}`. A diferencia de los hooks, devuelven datos (JSON), pero siguen sin devolver decisiones a ningún hook.
- La entrega por tmux reutiliza el adaptador `tmux.rs` (`send_keys`).
- Protocolo:
  - `ServerMsg::Mail { threads }` al cambiar algo.
  - Commands `approve_message`, `discard_message`, `set_thread_auto` y `pause_mail`.
- UI:
  - Sobre animado que viaja entre las dos oficinas.
  - Contador de mensajes en la oficina.
  - Panel de mensajes con hilos y aprobación.
- Instalación:
  - `mise run mcp:install` registra `kiti-mcp` en Claude (`claude mcp add -s user`), en Antigravity (`agy mcp add`) y en Codex (`~/.codex/config.toml`), con copia de seguridad.
  - `mcp:uninstall` lo quita.

### Dependencia nueva

`rmcp` 3.8: SDK oficial de MCP para Rust, del repositorio `modelcontextprotocol/rust-sdk`, con publicaciones frecuentes en 2026 y licencia Apache-2.0. Solo se usa en `crates/mcp`, con las features `server` y `transport-io`. La alternativa era escribir a mano el JSON-RPC de MCP (`initialize`, `tools/list`, `tools/call`), que habría que mantener al día con cada versión del protocolo.

## Seguridad y privacidad

- Todo es local: `127.0.0.1`, token y comprobación de `Host`, igual que los hooks.
- **El remitente lo pone Kiti** a partir del proceso, nunca el texto del mensaje.
- El texto de los mensajes vive solo en memoria del core y en la UI:
  - **nunca** se escribe en logs ni en disco (regla 6);
  - los logs solo registran ids, alias, longitudes y estados.
- **Inyección entre agentes:**
  - el texto entregado lleva siempre el prefijo `[Kiti] Mensaje de <alias>`;
  - las herramientas recuerdan que es información, no una orden del usuario;
  - el modo supervisado deja ver cada mensaje antes de entregarlo.
- **Archivos:** dos agentes en el mismo repositorio pueden pisarse. La documentación recomienda un `git worktree` por agente. Kiti no lo impone.

## Ficha de observabilidad

| Pregunta | Señal | Umbral y verificación |
|---|---|---|
| ¿Llegan los envíos? | `mail.sent` (alias, longitud, hilo, estado) | Cada `kiti_send` válido produce uno; test de integración del HookServer |
| ¿Se entregan? | `mail.delivered` y `mail.queued` con `wait_ms` | Entrega menos de 1 s después de aprobar si el destinatario está libre; prueba real con dos agentes |
| ¿Se rechaza lo que debe? | `mail.rejected` con `reason` (`unknown_sender`, `rate`, `hops`, `self`, `too_long`) | Tests de dominio |
| ¿Cuánto tráfico hay? | `mails_per_min` y `pending` en el panel de debug | Visible con `Cmd+Shift+D` |
| ¿Cuesta en la UI? | El test de rendimiento (50 agentes) con 10 sobres en vuelo | Dentro del presupuesto, `mise run ui:perf` |

## Plan

1. Maqueta con capturas y visto bueno del usuario.
2. Dominio `mailbox` (TDD) y endpoints del core.
3. `kiti-mcp` y el instalador.
4. Entrega por tmux, cola por turno y supervisión.
5. UI: sobre, contador y panel, con soporte en el simulador (`?mail=1`).
6. Prueba real corta con Claude y Antigravity, con permiso del usuario: un caso de revisión cruzada.
