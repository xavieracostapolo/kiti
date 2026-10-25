import { expect, test } from "@playwright/test";
import { openWorld } from "./helpers";

// Solo macOS + WebKit. Las baselines se fijan con el visto bueno del usuario (ADR 0006).
test.skip(process.platform !== "darwin", "baselines -webkit-darwin");

for (const zoom of ["far", "mid", "near"] as const) {
  test(`oficina simulada en zoom ${zoom}`, async ({ page }) => {
    await openWorld(page, `sim=20&seed=42&steps=357&freeze=1&anim=0&zoom=${zoom}`);
    await expect(page).toHaveScreenshot(`sim20-${zoom}.png`);
  });
}
