import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { viteSingleFile } from "vite-plugin-singlefile";

// SINGLEFILE=1 builds one self-contained HTML file for the web preview.
const singleFile = process.env.SINGLEFILE === "1";

export default defineConfig({
  plugins: [react(), tailwindcss(), ...(singleFile ? [viteSingleFile()] : [])],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    target: "es2022",
    outDir: singleFile ? "dist-preview" : "dist",
    chunkSizeWarningLimit: 2000,
  },
});
