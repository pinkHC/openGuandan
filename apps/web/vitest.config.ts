import { defineConfig } from "vitest/config";
import { resolve } from "node:path";

export default defineConfig({
  esbuild: { jsx: "automatic" },
  resolve: { alias: { "next/link": resolve(process.cwd(), "tests/next-link.tsx") } },
  test: {
    environment: "jsdom",
    setupFiles: ["./tests/setup.ts"],
  },
});
