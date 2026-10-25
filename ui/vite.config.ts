import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [react()],
  server: { fs: { allow: [".."] } },
  test: {
    environment: "node",
    include: ["src/**/*.test.{ts,tsx}"],
    setupFiles: ["src/test-setup.ts"],
    coverage: {
      provider: "v8",
      include: ["src/domain/**", "src/state/**", "src/transport/sim/**"],
      exclude: ["**/*.test.*"],
      thresholds: { lines: 90, functions: 90, branches: 85, statements: 90 },
    },
  },
});
