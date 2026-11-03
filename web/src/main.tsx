import App from "./app";
import "./index.css";
import "./styles/tokens.css";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

createRoot(document.querySelector("#root")!).render(
    <StrictMode>
        <App />
    </StrictMode>,
);
