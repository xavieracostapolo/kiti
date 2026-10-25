import { expect, test } from "@playwright/test";
import { openWorld } from "./helpers";

test("en reposo no se pinta ningún frame (2 s)", async ({ page }) => {
  await openWorld(page, "sim=20&seed=1&freeze=1&anim=0");
  const before = await page.evaluate(() => window.__kiti?.frames());
  await page.waitForTimeout(2000);
  const after = await page.evaluate(() => window.__kiti?.frames());
  expect(after).toBe(before);
  expect(await page.evaluate(() => window.__kiti?.metrics()["ui.ticker_running"])).toBe(false);
});

test("con la simulación activa, como mucho un render por paso", async ({ page }) => {
  await openWorld(page, "sim=20&seed=1&freeze=1&anim=0");
  const before = await page.evaluate(() => window.__kiti?.frames() ?? 0);
  for (let i = 0; i < 10; i++) {
    await page.evaluate(() => window.__kiti?.step(1));
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => r(null))));
  }
  const after = await page.evaluate(() => window.__kiti?.frames() ?? 0);
  expect(after - before).toBeLessThanOrEqual(10);
  expect(after - before).toBeGreaterThan(0);
});

test("en reposo no hay bucles de requestAnimationFrame (ni de Pixi)", async ({ page }) => {
  await page.addInitScript(() => {
    const w = window as Window & { __raf?: number };
    w.__raf = 0;
    const original = window.requestAnimationFrame.bind(window);
    window.requestAnimationFrame = (cb) => {
      w.__raf = (w.__raf ?? 0) + 1;
      return original(cb);
    };
  });
  await openWorld(page, "sim=20&seed=1&freeze=1&anim=0");
  await page.waitForTimeout(500);
  const raf = () => page.evaluate(() => (window as Window & { __raf?: number }).__raf ?? 0);
  const before = await raf();
  await page.waitForTimeout(2000);
  expect(await raf()).toBe(before);
});
