import { expect, test } from "@playwright/test";
import { openWorld } from "./helpers";

// ADR 0019: tablero de tareas. Con `?tasks=1`, el simulador trae una tarea de cada estado.
test("el chip abre el tablero con una fila por tarea", async ({ page }) => {
  await openWorld(page, "sim=20&seed=42&steps=357&freeze=1&anim=0&tasks=1");
  expect(await page.evaluate(() => window.__kiti?.tasks())).toEqual({
    active: 2,
    attention: 2,
    doneLast24h: 1,
  });
  await page.getByRole("button", { name: /Tablero de tareas/ }).click();
  const panel = page.getByRole("complementary", { name: "Tablero de tareas" });
  await expect(panel.getByRole("listitem")).toHaveCount(5);
  await expect(panel.getByRole("link", { name: /PR #12/ })).toHaveAttribute(
    "href",
    "https://github.com/ejemplo/tienda-web/pull/12",
  );
});

test("sin ?tasks no hay chip", async ({ page }) => {
  await openWorld(page, "sim=20&seed=42&steps=357&freeze=1&anim=0");
  await expect(page.getByRole("button", { name: /Tablero de tareas/ })).toHaveCount(0);
});

// Con `?mail=1` hay alias de oficina: cada tarea viva dibuja una línea del orquestador a su agente.
test("líneas y etiquetas de los agentes efímeros en el mapa", async ({ page }) => {
  await openWorld(page, "sim=20&seed=42&steps=357&freeze=1&anim=0&mail=1&tasks=1");
  // running, launching y stuck tienen oficina; done y failed no cuentan.
  expect(await page.evaluate(() => window.__kiti?.taskLinks())).toBe(3);
  const ids = (await page.evaluate(() => window.__kiti?.ids())) ?? [];
  const tags = await page.evaluate(
    (list) => list.map((id) => window.__kiti?.taskTag(id) ?? null),
    ids,
  );
  expect(tags.some((t) => t === "⚙ 3")).toBe(true);
  // De lejos, los agentes efímeros solo llevan el engranaje; de cerca, quién los lanzó y su rol.
  expect(tags.filter((t) => t === "⚙")).toHaveLength(3);
  await page.evaluate(() => window.__kiti?.setZoom("near"));
  const near = await page.evaluate(
    (list) => list.map((id) => window.__kiti?.taskTag(id) ?? null),
    ids,
  );
  expect(near).toContain("⚙ de claude-1 · implementar");
});

test("sin ?tasks no hay líneas", async ({ page }) => {
  await openWorld(page, "sim=20&seed=42&steps=357&freeze=1&anim=0&mail=1");
  expect(await page.evaluate(() => window.__kiti?.taskLinks())).toBe(0);
});
