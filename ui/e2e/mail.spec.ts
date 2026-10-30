import { expect, test } from "@playwright/test";
import { openWorld } from "./helpers";

// ADR 0017: mensajes entre agentes. Con `?mail=1`, el simulador trae un hilo de cada caso y uno
// pausado por el límite de saltos.
const Q = "sim=20&seed=42&steps=357&freeze=1&anim=0&mail=1";

test("alias, sobres y el panel de mensajes en automático", async ({ page }) => {
  await openWorld(page, Q);
  const ids = (await page.evaluate(() => window.__kiti?.ids())) ?? [];
  const tags = await page.evaluate(
    (list) => list.map((id) => window.__kiti?.mail(id) ?? null),
    ids,
  );
  expect(tags).toContain("claude-1");
  // El destinatario de la revisión tiene un sobre esperándole.
  expect(tags.some((t) => t?.startsWith("1 "))).toBe(true);
  expect(await page.evaluate(() => window.__kiti?.envelopes())).toBe(3);

  // Solo el hilo pausado pide atención.
  const chip = page.getByRole("button", { name: "Mensajes: 1 por decidir" });
  await chip.click();
  const panel = page.getByRole("complementary", { name: "Mensajes entre agentes" });
  await expect(panel.getByRole("button", { name: "Automático" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(panel.getByRole("region", { name: "Hilo Revisión" })).toContainText("en cola");
  await panel.getByRole("button", { name: "Continuar (+12)" }).click();
  await expect(
    page.getByRole("button", { name: /Mensajes entre agentes: \d+ en camino/ }),
  ).toBeVisible();
  // Lo retenido sigue su curso: pasa a la cola.
  await expect(panel.getByRole("region", { name: "Hilo Debate" })).toContainText("en cola");
});

test("al entregarse, el sobre sigue 3 s en el mapa y luego vuelve a 0 frames", async ({ page }) => {
  await openWorld(page, "sim=20&seed=42&steps=357&freeze=1&mail=1");
  const envelopes = () => page.evaluate(() => window.__kiti?.envelopes() ?? 0);
  await page.getByRole("button", { name: /^Mensajes/ }).click();
  const panel = page.getByRole("complementary", { name: "Mensajes entre agentes" });
  await panel.getByRole("button", { name: /Pausar/ }).click();
  await panel.getByRole("button", { name: /Reanudar/ }).click();
  // Los que esperaban ya están entregados, pero siguen a la vista.
  const during = await envelopes();
  expect(during).toBeGreaterThan(0);
  await page.waitForTimeout(300);
  expect(await page.evaluate(() => window.__kiti?.metrics()["ui.ticker_running"])).toBe(true);
  await page.waitForTimeout(3300);
  // Se fueron los entregados; quedan solo los que siguen esperando.
  expect(await envelopes()).toBeLessThan(during);
  const before = await page.evaluate(() => window.__kiti?.frames());
  await page.waitForTimeout(1000);
  expect(await page.evaluate(() => window.__kiti?.frames())).toBe(before);
});
