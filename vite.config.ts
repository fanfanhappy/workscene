import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath, URL } from "node:url";

// Tauri 需要固定端口，且开发时不清屏，避免吞掉 Rust 编译输出。
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: [
        "**/src-tauri/**",
        "**/.devdata/**",
        "**/.testdata/**",
        "**/dist-portable/**",
        "**/*.tmpdir/**",
        "**/*.tmpdir",
        "**/*.tmp",
      ],
    },
  },
  build: {
    target: "chrome110",
    sourcemap: false,
  },
});
