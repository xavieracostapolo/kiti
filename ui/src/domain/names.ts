// Nombre visible de una oficina: último segmento del cwd (lo calcula la UI, ADR 0004).

const MAX = 18;

export function displayName(cwd: string, max = MAX): string {
  if (cwd === "") return "?";
  const parts = cwd.split(/[/\\]/).filter((p) => p !== "");
  const last = parts.at(-1);
  if (last === undefined) return cwd;
  return last.length > max ? `${last.slice(0, max - 1)}…` : last;
}
