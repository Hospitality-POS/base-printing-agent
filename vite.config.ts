import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig(async () => ({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    proxy: {
      "/agent-api": {
        target: "https://api.hospitality.reliatech.co.ke",
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/agent-api/, ""),
      },
    },
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
