import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  // Tauri owns the terminal; don't let Vite wipe its output.
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { target: "es2022", emptyOutDir: true },
});
