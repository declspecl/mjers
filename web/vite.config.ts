import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// https://vite.dev/config/
export default defineConfig({
    plugins: [react()],
    server: {
        port: 5173,
        strictPort: true,
        // The local backend (`cargo run -p mjers-dev-server`) serves rooms on :3001 without CORS, so proxy it same-origin.
        proxy: {
            "/tables": "http://127.0.0.1:3001",
            "/ws": { target: "ws://127.0.0.1:3001", ws: true },
        },
    },
});
