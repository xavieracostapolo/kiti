# ADR 0018: Memoria de proyecto y de sesión

- **Fecha:** 2026-10-28
- **Estado:** Aceptada
- **Decisión:** Kiti actúa como **memoria compartida y de bajo consumo de contexto** para los agentes que corren en la máquina:
  1. **Memoria de trabajo de la sesión (automática, coste 0):** a partir de los hooks que ya llegan, cada agente conserva su **objetivo inicial** (primer pedido de la sesión), los **archivos modificados** (`Edit`, `Write`, `MultiEdit`) y su **checklist de tareas** (`TodoWrite`). No se borra cuando el CLI compacta su ventana de contexto.
  2. **Memoria del proyecto en SQLite + FTS5 (`bm25`):** en `memory.db` (modo `WAL`), Kiti persiste recuerdos estructurados por proyecto (`decision`, `gotcha`, `handoff`, `note`) e indexa su contenido con `FTS5`.
  3. **Recuperación en dos pasos por MCP (`kiti-mcp`):**
     - `kiti_recall()` sin consulta devuelve el **pulso e índice del proyecto** (~150 tokens): qué hacen o hicieron las sesiones del proyecto, qué archivos tocaron, qué tareas siguen pendientes y solo los **títulos** de los recuerdos guardados.
     - `kiti_recall(query)` busca en `FTS5` ordenando por `bm25()` y devuelve el detalle de los recuerdos más relevantes (hasta 5).
     - `kiti_remember(kind, title, detail, files?)` guarda un recuerdo en el proyecto del agente que llama.

## Contexto

Cuando una sesión de Claude Code, Codex o Antigravity se alarga, ocurren dos problemas:

1. **Compactación con amnesia:** al llenarse la ventana de contexto, el CLI resume la conversación y pierde detalles operativos clave (qué archivos ya se tocaron, qué pasos del plan estaban hechos, cuál era el objetivo inicial antes de diez mensajes de «sí / continúa» y qué trampas del entorno se descubrieron).
2. **Inflado de contexto:** meter todo el historial o documentos largos en cada turno consume miles de tokens y acelera la siguiente compactación.
3. **Silos entre agentes:** si un agente de otro proveedor (o una sesión nueva) entra al mismo proyecto, no sabe qué dejó a medias el anterior.

## Arquitectura elegida (CoALA + MemGPT/Letta sobre SQLite FTS5)

| Capa | Qué guarda | Cómo entra | Cómo sale (sin llenar el contexto) |
|---|---|---|---|
| **Memoria de Trabajo (`SessionMemory`)** | Objetivo inicial (`initial_prompt`), archivos tocados (`touched_files`, hasta 16) y checklist (`tasks`, hasta 16) | Automático desde los hooks `UserPromptSubmit` y `PreToolUse` (`Edit`, `Write`, `MultiEdit`, `TodoWrite`) | En el panel lateral de la oficina (`AgentPanel`) y en el pulso de `kiti_recall()` |
| **Memoria Semántica y Episódica (`ProjectMemory`)** | Hechos cortos por proyecto (`decision`, `gotcha`, `handoff`, `note`) con `title`, `detail` (≤ 600 chars) y `files` | `kiti_remember` desde cualquier agente del proyecto | Índice de títulos en `kiti_recall()`; detalle completo solo al buscar con `kiti_recall(query)` (`FTS5 BM25`, ≤ 5 resultados) |

### Por qué SQLite (`WAL` + `FTS5`) con `/usr/bin/sqlite3`

- Viene de serie en macOS con `FTS5` y `bm25()` compilados.
- No añade ninguna dependencia ni binario C al árbol de `Cargo.toml`, y mantiene el RSS de Kiti intacto (0 MB extra en reposo).
- Una consulta `FTS5 MATCH` sobre miles de notas tarda menos de 1 ms.
- `domain::memory` en `kiti-core` es 100 % puro y sin I/O; el port `MemoryStore` tiene una implementación en memoria (para tests y simulador) y el adaptador `SqliteMemoryStore` en `adapters::memory_sqlite` para la app nativa.

## Herramientas MCP (`kiti-mcp`)

| Herramienta | Entrada | Salida |
|---|---|---|
| `kiti_recall` | `query` opcional (texto a buscar) | Sin `query`: pulso del proyecto (sesiones, archivos tocados, tareas pendientes) + índice de títulos de recuerdos. Con `query`: hasta 5 recuerdos ordenados por relevancia (`FTS5 BM25`) |
| `kiti_remember` | `kind` (`decision` \| `gotcha` \| `handoff` \| `note`), `title` (≤ 120 chars), `detail` (≤ 600 chars), `files` opcional | Id del recuerdo guardado y confirmación |

- Igual que en el ADR 0017, `kiti-mcp` envía `X-Kiti-Ppid`; el core resuelve el agente y obtiene su `project` (raíz git o `cwd`). Un agente solo lee y escribe en la memoria de su propio proyecto.

## Privacidad y seguridad

- Nunca se guardan contenidos de archivos ni salidas de comandos: de `Edit`/`Write` solo se extrae el nombre relativo del archivo.
- Los logs (`tracing`) solo registran metadatos (`memory.stored`, `memory.recalled`, `kind`, `results`), nunca el título ni el detalle de los recuerdos (regla 6).

## Ficha de observabilidad

| Pregunta | Señal | Umbral y verificación |
|---|---|---|
| ¿Se guardan los recuerdos? | `memory.stored` (`kind`, `project_len`, `files`) | Cada `kiti_remember` válido emite uno; test de integración |
| ¿Responde rápido `kiti_recall`? | `memory.recalled` (`has_query`, `results`) | < 5 ms en SQLite FTS5; test de adaptador `memory_sqlite` |
| ¿Se captura el trabajo de la sesión por hooks? | `touched_files` y `tasks` en `AgentState` | Tests de `ingestion::claude` y `domain::agent` |
| ¿Se ve en el panel de debug? | `core.memories_total` en `CoreMetrics` (`Cmd+Shift+D`) y `window.__kiti.memory()` | Visible en modo sim y nativo |
