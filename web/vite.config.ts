import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// 開発サーバーは API を Axum バックエンドへプロキシする。
// 既定ポートは 8090（バックエンドの BIND_ADDR 既定と揃える）。
const API_TARGET = process.env.VITE_API_TARGET ?? "http://127.0.0.1:8090";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      "/api": {
        target: API_TARGET,
        changeOrigin: true,
        ws: true, // SSE/WebSocket を通す
      },
    },
  },
  build: {
    outDir: "dist",
    chunkSizeWarningLimit: 4000, // plotly が大きいため
  },
});
