import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
// Relative base so the built bundle works when served from the collector root.
// In dev, proxy API calls to the local collector so the app runs standalone.
export default defineConfig({
    plugins: [react()],
    base: "./",
    server: {
        port: 5173,
        proxy: {
            "/v1": "http://127.0.0.1:4357",
            "/healthz": "http://127.0.0.1:4357",
        },
    },
    build: {
        outDir: "dist",
        sourcemap: false,
    },
});
