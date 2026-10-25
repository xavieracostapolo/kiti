import { expect, test } from "@playwright/test";
import { openWorld } from "./helpers";

test("al pasar el ratón por una oficina aparece su nombre y estado", async ({ page }) => {
  await openWorld(page, "sim=20&seed=42&steps=357&freeze=1&anim=0");
  const c = await page.evaluate(() => window.__kiti?.screenCenter("2"));
  if (!c) throw new Error("sin oficina 2");
  await page.mouse.move(c.x, c.y);
  const tip = page.getByRole("tooltip");
  await expect(tip).toBeVisible();
  expect(await tip.textContent()).toMatch(/·/);
  // Fuera de cualquier oficina, desaparece.
  await page.mouse.move(2, 890);
  await expect(tip).toBeHidden();
});

test("en el simulador no hay botón de nuevo agente ni aviso de terminal", async ({ page }) => {
  await openWorld(page, "sim=20&seed=42&steps=357&freeze=1&anim=0");
  await expect(page.getByRole("button", { name: /nuevo agente/i })).toHaveCount(0);
  const c = await page.evaluate(() => window.__kiti?.screenCenter("2"));
  if (!c) throw new Error("sin oficina 2");
  await page.mouse.click(c.x, c.y);
  await expect(page.getByText(/abierto fuera de Kiti/i)).toHaveCount(0);
});
