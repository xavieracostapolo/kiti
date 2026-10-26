import { expect, test } from "@playwright/test";
import { openWorld } from "./helpers";

// ADR 0016: el tablero (sesión, git y última hora) solo aparece en el zoom cercano.
const Q = "sim=20&seed=42&steps=357&freeze=1&anim=0";

test("el tablero solo se ve al acercarse a la oficina", async ({ page }) => {
  await openWorld(page, Q);
  const claude = await page.evaluate(() =>
    window.__kiti?.ids().find((id) => id.startsWith("claude:")),
  );
  if (!claude) throw new Error("sin agentes de Claude");
  expect(await page.evaluate((id) => window.__kiti?.board(id), claude)).toBeNull();
  await page.evaluate(() => window.__kiti?.setZoom("mid"));
  await expect.poll(() => page.evaluate(() => window.__kiti?.lod())).toBe("mid");
  expect(await page.evaluate((id) => window.__kiti?.board(id), claude)).toBeNull();
  await page.evaluate((id) => window.__kiti?.focus(id), claude);
  await expect.poll(() => page.evaluate(() => window.__kiti?.lod())).toBe("near");
  const text = await page.evaluate((id) => window.__kiti?.board(id), claude);
  expect(text).toContain("contexto");
  expect(text).toContain("sesión");
  expect(text).toMatch(/main|feat\//);
});
