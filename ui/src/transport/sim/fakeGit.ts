// Estado de git falso de los proyectos del simulador (ADR 0016), estable por nombre.
import type { AgentState, ServerMsg } from "../../protocol";

const hash = (text: string) => [...text].reduce((h, c) => (h * 31 + c.charCodeAt(0)) >>> 0, 7);

export function fakeGit(agents: readonly AgentState[]): ServerMsg[] {
  const projects = [...new Set(agents.map((a) => a.project))].sort();
  return projects.map((project) => {
    const h = hash(project);
    return {
      type: "Git",
      project,
      git: {
        branch: h % 3 === 0 ? `feat/${project.split("/").at(-1)}` : "main",
        ahead: h % 4,
        behind: h % 5 === 0 ? 1 : 0,
        changed: h % 7,
      },
    };
  });
}
