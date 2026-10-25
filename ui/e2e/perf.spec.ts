import { expect, test } from "@playwright/test";
import { openWorld } from "./helpers";

// Solo en local, WebKit headed: los números de CI no son representativos (ADR 0003 §9).
test("50 agentes: p95 de frame ≤ 16.7 ms durante un recorrido de cámara", async ({ page }) => {
  await openWorld(page, "sim=50&seed=1&bench=pan");
  const r = await page.evaluate(() => window.__kiti?.bench(10_000));
  console.log(JSON.stringify(r));
  expect(r?.frames).toBeGreaterThan(100);
  expect(r?.p95).toBeLessThanOrEqual(16.7 + 1); // margen de 1 ms por la resolución del timer de rAF
  expect(r?.frameMsP95).toBeLessThanOrEqual(8);
});
