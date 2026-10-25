import { expect, test } from "@playwright/test";
import { openWorld } from "./helpers";

const Q = "sim=20&seed=42&steps=357&freeze=1&anim=0";

async function selectOffice(page: import("@playwright/test").Page, index: string) {
  const c = await page.evaluate((i) => window.__kiti?.screenCenter(i), index);
  if (!c) throw new Error(`sin oficina ${index}`);
  await page.mouse.click(c.x, c.y);
  await page.getByRole("button", { name: "Cerrar oficina" }).click();
  await expect(page.getByRole("alertdialog")).toBeVisible();
}

test("cerrar una oficina confirmada la hace desaparecer", async ({ page }) => {
  await openWorld(page, Q);
  await selectOffice(page, "2");
  await page.getByRole("alertdialog").getByRole("button", { name: "Cerrar oficina" }).click();
  await expect(page.getByText("19 agentes")).toBeVisible();
  await expect(page.getByRole("alertdialog")).toBeHidden();
});

test("cancelar no cierra nada", async ({ page }) => {
  await openWorld(page, Q);
  await selectOffice(page, "2");
  await page.getByRole("button", { name: "Cancelar" }).click();
  await expect(page.getByRole("alertdialog")).toBeHidden();
  await expect(page.getByText("20 agentes")).toBeVisible();
});
