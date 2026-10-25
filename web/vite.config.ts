import { defineConfig } from "vitest/config";
import vue from "@vitejs/plugin-vue";

export default defineConfig({
  plugins: [vue()],
  resolve: { dedupe: ["vue"] },
  server: {
    // In development the runner is a separate process; its API is proxied so the page and the API share an origin,
    // as they do when the runner serves the built UI.
    proxy: { "/api": { target: "http://127.0.0.1:7878", changeOrigin: false } },
  },
  build: { target: "es2022" },
  test: { include: ["tests/**/*.test.ts"], environment: "happy-dom" },
});
