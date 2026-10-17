import { defineConfig } from "vite";

// Tauri conventions: fixed port, no screen clearing (keeps Rust errors visible).
declare const process: { env: Record<string, string | undefined> };
const host: string | undefined = process.env.TAURI_DEV_HOST;

export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**", "**/target/**", "**/FlowJo VX/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    outDir: "dist",
    target: "safari15",
    sourcemap: false,
    chunkSizeWarningLimit: 800,
  },
});
