import { fileURLToPath } from "node:url";

/** @type { import("prettier").Config } */
const config = {
    trailingComma: "all",
    tabWidth: 4,
    useTabs: false,
    semi: true,
    singleQuote: false,
    printWidth: 140,
    plugins: [fileURLToPath(import.meta.resolve("@trivago/prettier-plugin-sort-imports"))],
};

export default config;
