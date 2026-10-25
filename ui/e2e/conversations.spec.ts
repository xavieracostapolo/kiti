import { expect, test } from "@playwright/test";
import { openWorld } from "./helpers";

// Hito 11 (ADR 0009): zonas por proyecto y conversaciones cerradas como oficinas apagadas.
const Q = "sim=20&seed=42&steps=357&freeze=1&anim=0&conversations=1";

test("las oficinas se agrupan en zonas por proyecto", async ({ page }) => {
  await openWorld(page, Q);
  await expect.poll(() => page.evaluate(() => window.__kiti?.dormant().length)).toBe(4);
  const zones = await page.evaluate(() => window.__kiti?.zones() ?? []);
  // 20 agentes en 10 proyectos y una zona más solo con conversaciones cerradas.
  expect(zones.map((z) => z.name)).toContain("thoth-assets");
  expect(zones.find((z) => z.name === "thoth-assets")?.count).toBe(2);
  expect(zones.reduce((n, z) => n + z.count, 0)).toBe(24);
});

test("una oficina apagada dice qué era y se retoma desde su panel", async ({ page }) => {
  await openWorld(page, Q);
  await expect.poll(() => page.evaluate(() => window.__kiti?.dormant().length)).toBe(4);
  const id = await page.evaluate(() => window.__kiti?.dormant()[0]);
  const c = await page.evaluate((i) => window.__kiti?.screenCenter(i ?? ""), id);
  if (!c) throw new Error("sin oficina apagada");

  await page.mouse.move(c.x, c.y);
  const tip = page.getByRole("tooltip");
  await expect(tip).toContainText("Revisar vulnerabilidades");
  await expect(tip).toContainText("Clic para retomarla");

  await page.mouse.click(c.x, c.y);
  const panel = page.getByRole("complementary", { name: "Conversación cerrada" });
  await expect(panel).toContainText("revisa las dependencias con CVE abiertos");
  await panel.getByRole("button", { name: "Retomar" }).click();

  // Como al lanzar: la oficina del agente queda seleccionada y la apagada desaparece.
  await expect(page.getByRole("complementary", { name: "Agente seleccionado" })).toBeVisible();
  await expect.poll(() => page.evaluate(() => window.__kiti?.dormant().length)).toBe(3);
});

test("el detalle de un agente de Claude dice en qué trabaja", async ({ page }) => {
  await openWorld(page, Q);
  const claude = await page.evaluate(() =>
    window.__kiti?.ids().find((id) => id.startsWith("claude:")),
  );
  if (!claude) throw new Error("sin agentes de Claude");
  await page.evaluate((i) => window.__kiti?.focus(i), claude);
  const c = await page.evaluate((i) => window.__kiti?.screenCenter(i), claude);
  if (!c) throw new Error("sin oficina");
  await page.mouse.click(c.x, c.y);
  const panel = page.getByRole("complementary", { name: "Agente seleccionado" });
  await expect(panel).toContainText("último pedido");
});
