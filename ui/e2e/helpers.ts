import { expect, type Page } from "@playwright/test";

/** Id de la oficina que está más a un lado de la pantalla (las zonas mueven los índices). */
export async function officeAt(page: Page, side: "topLeft" | "right"): Promise<string> {
  const id = await page.evaluate((s) => {
    const k = window.__kiti;
    if (!k) return undefined;
    const score = (c: { x: number; y: number }) => (s === "right" ? c.x : -(c.x + c.y));
    let best: [string, number] | undefined;
    for (const id of k.ids()) {
      const c = k.screenCenter(id);
      if (c && (!best || score(c) > best[1])) best = [id, score(c)];
    }
    return best?.[0];
  }, side);
  if (!id) throw new Error("sin oficinas");
  return id;
}

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
