import "./index.css";
import { router } from "./router";
import "./styles/tokens.css";
import { RouterProvider } from "@tanstack/react-router";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

createRoot(document.querySelector("#root")!).render(
    <StrictMode>
        <RouterProvider router={router} />
    </StrictMode>,
);
