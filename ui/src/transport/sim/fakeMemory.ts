// Recuerdos de proyecto falsos para el modo sim (ADR 0018), estables por proyecto.
import type { AgentState, MemoryItem, ServerMsg } from "../../protocol";

export function fakeMemory(agents: readonly AgentState[], now: number): ServerMsg {
  const projects = [...new Set(agents.map((a) => a.project))].sort();
  const memories: MemoryItem[] = [];
  let nextId = 0;
  for (const project of projects) {
    const short = project.split("/").at(-1) ?? project;
    memories.push(
      {
        id: ++nextId,
        project,
        kind: "decision",
        title: `Usar SQLite FTS5 con BM25 en ${short}`,
        detail:
          "Búsqueda en < 1 ms y recuperación en 2 pasos (índice ligero + detalle bajo demanda).",
        files: ["crates/core/src/adapters/memory_sqlite.rs"],
        author: "claude-1",
        createdAt: now - 180_000,
      },
      {
        id: ++nextId,
        project,
        kind: "gotcha",
        title: "No releer JSONL completos en el tailer",
        detail: "Leer siempre desde el último offset para mantener CPU < 1 % en reposo.",
        files: ["crates/core/src/adapters/tail.rs"],
        author: "agy-1",
        createdAt: now - 90_000,
      },
    );
  }
  return { type: "Memory", memories };
}
