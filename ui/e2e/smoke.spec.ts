import { expect, test } from "@playwright/test";
import { openWorld } from "./helpers";

test("arranca con 20 oficinas y sin errores de consola", async ({ page }) => {
  const errors = await openWorld(page, "sim=20&seed=42&steps=357&freeze=1&anim=0");
  const counts = await page.evaluate(() => window.__kiti?.counts());
  expect(Object.values(counts ?? {}).reduce((a, b) => a + b, 0)).toBe(20);
  expect(await page.getByText("20 agentes").isVisible()).toBe(true);
  expect(errors).toEqual([]);
});

test("con la semilla fijada aparecen los cinco colores", async ({ page }) => {
  await openWorld(page, "sim=20&seed=42&steps=357&freeze=1&anim=0");
  const counts = await page.evaluate(() => window.__kiti?.counts());
  for (const kind of ["working", "waiting", "idle", "error", "dead"] as const) {
    expect(counts?.[kind], kind).toBeGreaterThan(0);
  }
});
