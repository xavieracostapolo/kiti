import { expect, type Page } from "@playwright/test";

export async function openWorld(page: Page, query: string): Promise<string[]> {
  const errors: string[] = [];
  page.on("console", (m) => {
    if (m.type() === "error") errors.push(m.text());
  });
  page.on("pageerror", (e) => errors.push(String(e)));
  await page.goto(`/?${query}`);
  await expect(page.locator('[data-world-ready="true"]')).toBeVisible();
  return errors;
}
