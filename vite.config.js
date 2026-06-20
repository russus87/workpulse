import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Configurazione Vite pensata per Tauri:
// - porta fissa 1430 (la stessa indicata in tauri.conf.json)
// - non pulisce lo schermo, cosi' i log di Tauri restano visibili
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1430,
    strictPort: true,
    host: "localhost",
  },
});
