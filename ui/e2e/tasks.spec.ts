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
