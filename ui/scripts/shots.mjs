// Capturas del ciclo visual (ADR 0006, Hito 1). Requiere `mise run ui:dev` levantado.
// Uso: pnpm -C ui shots [--base=http://127.0.0.1:5173] [--sim=20] [--seed=42] [--extra=conversations=1]
import { mkdirSync } from "node:fs";
import { webkit } from "@playwright/test";

const arg = (name, fallback) =>
  process.argv.find((a) => a.startsWith(`--${name}=`))?.split("=")[1] ?? fallback;

const base = arg("base", "http://127.0.0.1:5173");
const sim = arg("sim", "20");
const seed = arg("seed", "42");
// steps=357: con seed 42 conviven los cinco colores (el error dura poco).
const extra = process.argv.find((a) => a.startsWith("--extra="))?.slice("--extra=".length);
const common = `sim=${sim}&seed=${seed}&steps=357&freeze=1&anim=0${extra ? `&${extra}` : ""}`;
const shots = [
  ["far", `${common}&zoom=far`],
  ["mid", `${common}&zoom=mid`],
  ["near", `${common}&zoom=near`],
  ["focus3", `${common}&focus=3`],
];

mkdirSync(".shots", { recursive: true });
const browser = await webkit.launch();
let failed = false;
for (const [name, query] of shots) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  const errors = [];
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  page.on("pageerror", (e) => errors.push(String(e)));
  await page.goto(`${base}/?${query}`);
  await page.waitForSelector('[data-world-ready="true"]', { timeout: 15_000 });
  const counts = await page.evaluate(() => window.__kiti?.counts());
  const file = `.shots/sim${sim}-${name}.png`;
  await page.screenshot({ path: file });
  console.log(`${file}  counts=${JSON.stringify(counts)}  errors=${errors.length}`);
  for (const e of errors) console.error(`  console.error: ${e}`);
  if (errors.length) failed = true;
  await page.close();
}
await browser.close();
process.exit(failed ? 1 : 0);
