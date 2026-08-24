import { defineConfig } from "vite";

// WebDesk 管理控制台前端（Tauri 前端）
// dev 端口固定 1420（与 tauri.conf.json devUrl 一致）
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2022",
  },
});
