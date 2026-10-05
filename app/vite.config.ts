import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": "/src",
    },
  },
  server: {
    host: "127.0.0.1",
    port: 5173,
    proxy: {
      "/api": "http://127.0.0.1:5174",
      "/data": "http://127.0.0.1:5174",
      "/layouts": "http://127.0.0.1:5174",
      "/raw-files": "http://127.0.0.1:5174",
      "/version": "http://127.0.0.1:5174",
      "/zone_names": "http://127.0.0.1:5174",
    },
  },
});
