# ADR 0016: Tablero de la oficina (solo en el zoom cercano)

- **Fecha:** 2026-10-25
- **Estado:** Aceptada
- **Decisión:** al acercarse a una oficina aparece un tablero con tres datos:
  - **sesión:** modelo, % de contexto usado y coste (Claude);
  - **git del proyecto:** rama, archivos cambiados y commits por subir o por bajar;
  - **línea de tiempo de la última hora:** trabajando, esperando o inactivo.

  Solo se ve en el nivel de detalle `near`, para no recargar la vista general ni la media (petición del usuario).

## Contexto

Entre las ideas de productividad, el usuario eligió las de visibilidad, con una condición: que no sobrecarguen la vista general.

## Decisiones

- **Sesión (Claude):**
  - Sale del mismo JSON de la status line que ya llega para la cuota (ADR 0015): `session_id`, `model.display_name`, `context_window.used_percentage` y `cost.total_cost_usd`.
  - El core enlaza `session_id` con su agente, como hace con los hooks.
  - El coste viaja en céntimos (`AgentState` es `Eq`).
  - Antigravity no da estos datos y su tablero los omite.
- **Git:**
  - Cada 15 s, el shell pide al runtime los proyectos con agentes vivos y ejecuta en cada uno `git --no-optional-locks status --porcelain=v2 --branch`. `--no-optional-locks` evita que la consulta coja `index.lock` y choque con los commits de los agentes.
  - El runtime guarda el estado de cada proyecto, envía `ServerMsg::Git` y lo reenvía tras cada Snapshot, igual que la cuota.
  - Las carpetas sin git no muestran nada.
- **Línea de tiempo:**
  - Cada agente guarda sus últimos cambios de estado (hasta 32, en `history`).
  - La UI pinta la última hora en una barra de colores con el semáforo.
- **Dónde:** un panel en la parte izquierda del suelo, dentro del contenedor `near` de `OfficeView`. Fuera de ese zoom ni se ve ni se dibuja.

## Privacidad

Modelo, contexto, coste y estado de git son metadatos locales; no se registran en los logs.

## Ficha de observabilidad

| Pregunta | Señal | Verificación |
|---|---|---|
| ¿Llegan los datos de sesión? | La oficina de una sesión de Claude muestra modelo, contexto y coste en el zoom cercano | Prueba real tras una respuesta |
| ¿Funciona git? | `git.updated` (debug) con `projects` y `duration_ms` | Menos de 50 ms por ronda con 3 proyectos |
| ¿Cuesta al zoom? | El test de rendimiento (50 agentes) sigue en el presupuesto | `mise run ui:perf` |

## Resultados

- En el zoom cercano, la oficina muestra el tablero en el suelo, bajo la insignia del proveedor:
  - modelo,
  - «contexto N %»,
  - «$X.XX sesión»,
  - rama · cambios · ↑/↓,
  - la barra de la última hora.

  En las vistas media y lejana no se ve ni se dibuja. Lo comprueba el E2E `board.spec.ts` con `window.__kiti.board(id)`.
- **Coste:** el texto del tablero solo se recalcula cuando está visible (igual que el título y la nota del ADR 0011). Con 50 agentes, el coste por frame en p95 sigue en 2-3 ms. El p95 entre frames dio 18 ms tanto en esta rama como en `main` con la máquina cargada (load average ~3,9), así que es el límite del timer de WebKit y no una regresión.
- El simulador da modelo, contexto y coste a las oficinas de Claude y un git ficticio por proyecto, para las capturas y los tests.

