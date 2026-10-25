// Texto dentro de las oficinas que se adapta al zoom: al acercarse, la letra se hace más pequeña
// en el mundo (y similar en pantalla), así caben más caracteres y más líneas. Puro.

/** Ancho de un carácter de la fuente monoespaciada, en fracciones de su tamaño. */
export const CHAR_W = 0.6;
/** Alto de línea, en fracciones del tamaño de letra. */
export const LINE_H = 1.25;

/**
 * Tamaño de letra en px de mundo para que en pantalla mida `screenPx` con la escala `k`, entre
 * `min` y `max`. Se cuantiza a medios píxeles: el zoom continuo no recalcula el texto en cada frame.
 */
export function fontFor(k: number, screenPx: number, min: number, max: number): number {
  const size = Math.floor((screenPx / k) * 2) / 2;
  return Math.min(max, Math.max(min, size));
}

/** Caracteres que caben en `width` px de mundo con letra `size`. */
export const charsIn = (width: number, size: number) =>
  Math.max(1, Math.floor(width / (size * CHAR_W)));

/**
 * Parte `text` en como mucho `maxLines` líneas de `perLine` caracteres, por palabras. Lo que no
 * cabe se corta con `…` al final de la última línea.
 */
export function fitLines(text: string, perLine: number, maxLines: number): string {
  const words = text.split(/\s+/).filter((w) => w !== "");
  const lines: string[] = [];
  let line = "";
  let i = 0;
  while (i < words.length && lines.length < maxLines - 1) {
    const word = words[i] as string;
    if (line === "" && word.length > perLine) {
      lines.push(word.slice(0, perLine));
      words[i] = word.slice(perLine);
      continue;
    }
    const next = line === "" ? word : `${line} ${word}`;
    if (next.length <= perLine) {
      line = next;
      i++;
    } else {
      lines.push(line);
      line = "";
    }
  }
  const rest = [line, ...words.slice(i)].filter((w) => w !== "").join(" ");
  if (rest !== "") lines.push(rest.length > perLine ? `${rest.slice(0, perLine - 1)}…` : rest);
  return lines.join("\n");
}
