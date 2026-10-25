import { execSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { extname, join } from "node:path";
import { expect, type Page, test } from "@playwright/test";
import { openWorld } from "./helpers";

const Q = "sim=20&seed=42&steps=357&freeze=1&anim=0&terminals=1";

async function openTerminalOfFirstOffice(page: Page) {
  const c = await page.evaluate(() => window.__kiti?.screenCenter("0"));
  if (!c) throw new Error("sin oficina 0");
  await page.mouse.click(c.x, c.y);
  const panel = page.getByRole("region", { name: "Terminal del agente" });
  await expect(panel).toBeVisible();
  await expect(panel.locator(".xterm-rows")).toContainText("Terminal simulada de Kiti");
  return panel;
}

test("clic en una oficina con terminal la abre y se puede escribir", async ({ page }) => {
  await openWorld(page, Q);
  const panel = await openTerminalOfFirstOffice(page);
  await page.keyboard.type("hola-kiti");
  await expect(panel.locator(".xterm-rows")).toContainText("hola-kiti");
  await panel.getByRole("button", { name: "Cerrar" }).click();
  await expect(panel).toBeHidden();
});

// La CSP de producción de tauri.conf.json, aplicada al build real (Vite en desarrollo inyecta
// estilos y scripts inline y no serviría para comprobarla).
test("la terminal funciona con la CSP de producción", async ({ page }) => {
  const ui = join(import.meta.dirname, "..");
  // Siempre se reconstruye: un dist viejo probaría otro código.
  execSync("pnpm build", { cwd: ui, stdio: "ignore" });
  const conf = JSON.parse(readFileSync(join(ui, "../src-tauri/tauri.conf.json"), "utf8"));
  const csp = Object.entries(conf.app.security.csp as Record<string, string>)
    .map(([k, v]) => `${k} ${v}`)
    .join("; ");
  const types: Record<string, string> = {
    ".html": "text/html",
    ".js": "text/javascript",
    ".css": "text/css",
    ".png": "image/png",
    ".svg": "image/svg+xml",
  };
  const violations: string[] = [];
  page.on("console", (m) => {
    if (/Content Security Policy|Refused to/i.test(m.text())) violations.push(m.text());
  });
  await page.route("http://kiti.test/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    const file = join(ui, "dist", path === "/" ? "index.html" : path);
    if (!existsSync(file)) return route.fulfill({ status: 404 });
    await route.fulfill({
      body: readFileSync(file),
      headers: {
        "content-type": types[extname(file)] ?? "application/octet-stream",
        "content-security-policy": csp,
      },
    });
  });
  await page.goto(`http://kiti.test/?${Q}`);
  await expect(page.locator('[data-world-ready="true"]')).toBeVisible();
  const panel = await openTerminalOfFirstOffice(page);
  await page.keyboard.type("csp-ok");
  await expect(panel.locator(".xterm-rows")).toContainText("csp-ok");
  expect(violations).toEqual([]);
});
