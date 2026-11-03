import jseslint from "@eslint/js";
import eslintConfigPrettier from "eslint-config-prettier";
import pluginReact from "eslint-plugin-react";
import pluginReactHooks from "eslint-plugin-react-hooks";
import eslintPluginUnicorn from "eslint-plugin-unicorn";
import { defineConfig, globalIgnores } from "eslint/config";
import globals from "globals";
import tseslint from "typescript-eslint";

const allCaps = "/(^|[^A-Za-z])[A-Z]{2,}([^A-Za-z]|$)/";
const allCapsMessage = "Do not write text in all caps. Use sentence case.";

export default defineConfig(
    jseslint.configs.recommended,
    tseslint.configs.recommended,
    eslintPluginUnicorn.configs.recommended,
    pluginReact.configs.flat.recommended,
    globalIgnores(["dist", "node_modules", ".cache", ".env*", "pnpm-*"]),
    {
        languageOptions: { globals: globals.browser },
        settings: { react: { version: "detect" } },
        plugins: { "react-hooks": pluginReactHooks },
        rules: {
            ...pluginReactHooks.configs.recommended.rules,
            "react/react-in-jsx-scope": "off",
            "unicorn/no-null": "off",
            "unicorn/no-array-reduce": "off",
            "unicorn/no-useless-undefined": "off",
            "unicorn/prevent-abbreviations": "off",
            "unicorn/no-for-loop": "error",
            "@typescript-eslint/no-unused-vars": "warn",
            "@typescript-eslint/no-explicit-any": "error",
            "@typescript-eslint/consistent-type-imports": "error",
            curly: ["error", "all"],
            "padding-line-between-statements": ["error", { blankLine: "always", prev: "*", next: "return" }],
            "no-restricted-syntax": [
                "error",
                { selector: `JSXText[value=${allCaps}]`, message: allCapsMessage },
                { selector: `JSXAttribute > Literal[value=${allCaps}]`, message: allCapsMessage },
                { selector: `JSXExpressionContainer > Literal[value=${allCaps}]`, message: allCapsMessage },
                { selector: `JSXExpressionContainer TemplateElement[value.cooked=${allCaps}]`, message: allCapsMessage },
                {
                    selector: "TSAsExpression[typeAnnotation.type='TSUnknownKeyword']",
                    message: "Do not use 'as unknown'. Fix the types strictly.",
                },
                {
                    selector: "TSAsExpression > TSAsExpression",
                    message: "Do not use double assertions. Fix the types strictly.",
                },
            ],
        },
    },
    {
        files: ["src/**/*.tsx"],
        rules: { "unicorn/filename-case": ["error", { cases: { kebabCase: true, pascalCase: true } }] },
    },
    eslintConfigPrettier,
);
