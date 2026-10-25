import { expect, test } from "@playwright/test";
import { openWorld } from "./helpers";

// Hito 13: los contadores y el buscador filtran; lo demás se atenúa y la cámara encuadra.
const Q = "sim=20&seed=42&steps=357&freeze=1&anim=0&conversations=1";

test("pulsar «esperando» atenúa el resto y acerca la cámara a esas oficinas", async ({ page }) => {
  await openWorld(page, Q);
  const before = await page.evaluate(() => window.__kiti?.scale() ?? 0);
  await page.getByTestId("count-waiting").click();
  const counts = await page.evaluate(() => window.__kiti?.counts());
  const total = 20 + 4; // agentes + apagadas
  await expect
    .poll(() => page.evaluate(() => window.__kiti?.dimmed().length))
    .toBe(total - (counts?.waiting ?? 0));
  expect(await page.evaluate(() => window.__kiti?.scale() ?? 0)).toBeGreaterThan(before);
  await page.getByRole("button", { name: "Quitar filtro" }).click();
  await expect.poll(() => page.evaluate(() => window.__kiti?.dimmed().length)).toBe(0);
});

test("el buscador encuentra por proyecto, también las apagadas", async ({ page }) => {
  await openWorld(page, Q);
  await page.keyboard.press("/");
  await page.keyboard.type("thoth");
  // Solo quedan las dos conversaciones cerradas de thoth-assets.
  await expect.poll(() => page.evaluate(() => window.__kiti?.dimmed().length)).toBe(22);
  await page.keyboard.press("Escape");
  await expect.poll(() => page.evaluate(() => window.__kiti?.dimmed().length)).toBe(0);
});
