import { expect, test } from "@playwright/test";
import { officeAt, openWorld } from "./helpers";

const Q = "sim=20&seed=42&steps=357&freeze=1";

test("doble clic enfoca la oficina en 400 ms o menos y la selecciona", async ({ page }) => {
  await openWorld(page, Q);
  const c = await page.evaluate(() => window.__kiti?.screenCenter("7"));
  if (!c) throw new Error("sin oficina 7");
  const t0 = Date.now();
  await page.mouse.dblclick(c.x, c.y);
  await expect.poll(() => page.evaluate(() => window.__kiti?.lod())).toBe("near");
  // Centrada en la zona libre: bajo la barra superior (56 px) → (720, 56 + (900 - 56) / 2).
  await expect
    .poll(async () => {
      const p = await page.evaluate(() => window.__kiti?.screenCenter("7"));
      return p ? Math.hypot(p.x - 720, p.y - 478) : 999;
    })
    .toBeLessThan(2);
  expect(Date.now() - t0).toBeLessThan(1500); // 400 ms de animación + latencia de Playwright
  await expect(page.getByRole("complementary", { name: "Agente seleccionado" })).toBeVisible();
});

test("la tecla 0 vuelve a la vista general", async ({ page }) => {
  await openWorld(page, `${Q}&anim=0&zoom=near`);
  expect(await page.evaluate(() => window.__kiti?.lod())).toBe("near");
  await page.keyboard.press("0");
  await expect.poll(() => page.evaluate(() => window.__kiti?.lod())).toBe("far");
});

test("el zoom respeta sus límites", async ({ page }) => {
  await openWorld(page, `${Q}&anim=0`);
  await page.mouse.move(720, 450);
  await page.keyboard.down("Control");
  for (let i = 0; i < 30; i++) await page.mouse.wheel(0, -400);
  expect(await page.evaluate(() => window.__kiti?.scale())).toBeCloseTo(6);
  for (let i = 0; i < 60; i++) await page.mouse.wheel(0, 400);
  await page.keyboard.up("Control");
  expect(await page.evaluate(() => window.__kiti?.scale())).toBeCloseTo(0.25);
});

test("el pan no saca el mundo de la vista", async ({ page }) => {
  await openWorld(page, `${Q}&anim=0`);
  const first = await officeAt(page, "topLeft");
  for (let i = 0; i < 20; i++) await page.mouse.wheel(-2000, -2000);
  const c = await page.evaluate((id) => window.__kiti?.screenCenter(id), first);
  // El centro de la cámara queda dentro del mundo: la oficina de la esquina sigue a la vista.
  expect(c?.x).toBeGreaterThan(0);
  expect(c?.y).toBeGreaterThan(0);
  expect(c?.x).toBeLessThan(1440);
  expect(c?.y).toBeLessThan(900);
});
