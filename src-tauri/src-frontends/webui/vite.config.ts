import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  build: {
    // react-data-grid 的 light-dark() 变量在 Vite 8 的 lightningcss 压缩器中
    // 可能被错误改写，使用现代 CSS 目标保留 DataGrid 的主题变量。
    cssTarget: "esnext",
  },
  server: {
    port: 11000,
    strictPort: true,
  },
  clearScreen: false,
});
