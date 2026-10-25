import { expect, type Page, test } from "@playwright/test";
import { officeAt, openWorld } from "./helpers";

const Q = "sim=20&seed=42&steps=357&freeze=1&anim=0&terminals=1";
const terminal = (page: Page) => page.getByRole("region", { name: "Terminal del agente" });

test("Nuevo agente enfoca y selecciona su oficina sin abrir la terminal", async ({ page }) => {
  await openWorld(page, Q);
  await page.getByRole("button", { name: /nuevo agente/i }).click();
  await page.getByRole("menuitem", { name: "Claude Code" }).click();
  // La oficina del agente lanzado (índice 1 en el simulador) queda seleccionada y enfocada.
  await expect(page.getByRole("complementary", { name: "Agente seleccionado" })).toBeVisible();
  await expect.poll(() => page.evaluate(() => window.__kiti?.lod())).toBe("near");
  await expect(terminal(page)).toBeHidden();
  await expect(page.getByRole("button", { name: "Abrir terminal" })).toBeVisible();
});

test("clic en una oficina con terminal la abre sin taparla y cerrar la deja seleccionada", async ({
  page,
}) => {
  await openWorld(page, Q);
  // La oficina más a la derecha queda donde se abrirá el panel.
  const right = await officeAt(page, "right");
  const before = await page.evaluate((id) => window.__kiti?.screenCenter(id), right);
  const c = await page.evaluate(() => window.__kiti?.screenCenter("0"));
  if (!c || !before) throw new Error("sin oficinas");
  await page.mouse.click(c.x, c.y);
  await expect(terminal(page)).toBeVisible();
  // Todas las oficinas se reencuadran a la izquierda del panel: también esa.
  const panel = await terminal(page).boundingBox();
  expect(before.x).toBeGreaterThan(panel?.x ?? 0);
  await expect
    .poll(
      async () => (await page.evaluate((id) => window.__kiti?.screenCenter(id), right))?.x ?? 9999,
    )
    .toBeLessThan(panel?.x ?? 0);
  await terminal(page).getByRole("button", { name: "Cerrar" }).click();
  await expect(terminal(page)).toBeHidden();
  await expect(page.getByRole("button", { name: "Abrir terminal" })).toBeVisible();
});

test("clic en una oficina sin terminal cierra la abierta y muestra su panel", async ({ page }) => {
  await openWorld(page, Q);
  const c0 = await page.evaluate(() => window.__kiti?.screenCenter("0"));
  if (!c0) throw new Error("sin oficina 0");
  await page.mouse.click(c0.x, c0.y);
  await expect(terminal(page)).toBeVisible();
  const c2 = await page.evaluate(() => window.__kiti?.screenCenter("2"));
  if (!c2) throw new Error("sin oficina 2");
  await page.mouse.click(c2.x, c2.y);
  await expect(terminal(page)).toBeHidden();
  await expect(page.getByText(/abierto fuera de Kiti/i)).toBeVisible();
});
