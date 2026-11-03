import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// https://vite.dev/config/
export default defineConfig({
    plugins: [react()],
    server: {
        port: 5173,
        strictPort: true,
        fs: {
            // The engine package is linked from ../npm, outside this root.
            allow: [".."],
        },
    },
});
