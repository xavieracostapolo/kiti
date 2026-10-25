import { defineConfig, devices } from "@playwright/test";

// Dónde corre cada proyecto (ADR 0006 §2): chromium en cualquier SO (funcional, sin capturas);
// webkit-visual solo en macOS (baselines -webkit-darwin); webkit-perf solo en local, headed.
export default defineConfig({
  testDir: "e2e",
  fullyParallel: true,
  reporter: process.env.CI ? "github" : "list",
  use: { baseURL: "http://127.0.0.1:5173", viewport: { width: 1440, height: 900 } },
  snapshotPathTemplate:
    "{testDir}/__screenshots__/{testFilePath}/{arg}-{projectName}-{platform}{ext}",
  webServer: {
    command: "pnpm dev --host 127.0.0.1 --port 5173 --strictPort",
    url: "http://127.0.0.1:5173",
    reuseExistingServer: true,
  },
  projects: [
    {
      name: "chromium",
      testIgnore: /(visual|perf)\.spec\.ts/,
      use: { ...devices["Desktop Chrome"], viewport: { width: 1440, height: 900 } },
    },
    {
      name: "webkit-visual",
      testMatch: /visual\.spec\.ts/,
      use: { ...devices["Desktop Safari"], viewport: { width: 1440, height: 900 } },
    },
    {
      name: "webkit-perf",
      testMatch: /perf\.spec\.ts/,
      timeout: 60_000,
      use: {
        ...devices["Desktop Safari"],
        viewport: { width: 1440, height: 900 },
        headless: false,
      },
    },
  ],
});
