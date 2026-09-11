import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri startet den Dev-Server selbst; der feste Port und `strictPort` sorgen
// dafuer, dass `devUrl` in tauri.conf.json stimmt statt auf einen
// Ausweichport zu zeigen.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    // Ohne das laeuft der Watcher in `src-tauri` mit und startet bei jedem
    // Rust-Build neu.
    watch: { ignored: ["**/src-tauri/**"] },
  },
  // Android laeuft auf aelteren WebViews als der Desktop.
  build: {
    target: "es2021",
    minify: !process.env.TAURI_ENV_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
});
