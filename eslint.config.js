// FilePath: eslint.config.js

// Suppression policy: inline disable-directive comments and every other inline configuration
// comment are forbidden and inert by design. `noInlineConfig` makes a directive do nothing, and
// `reportUnusedDisableDirectives` fails the lint the moment one appears, so a directive can
// never lie to the next reader. scripts/guard/no-suppressions.sh rejects them as well. If a
// rule is wrong for a real reason, fix the code or change this file; never silence a line.

import js from "@eslint/js";
import globals from "globals";
import reactHooks from "eslint-plugin-react-hooks";
import tseslint from "typescript-eslint";
import { defineConfig } from "eslint/config";

export default defineConfig(
    {
        ignores: [
            "dist/",
            "node_modules/",
            "target/",
            "src-tauri/",
            "engine/",
            "crates/",
            "eslint.config.js",
        ],
    },
    {
        files: ["**/*.{ts,tsx}"],
        extends: [
            js.configs.recommended,
            ...tseslint.configs.strictTypeChecked,
            ...tseslint.configs.stylisticTypeChecked,
            reactHooks.configs.flat.recommended,
        ],
        linterOptions: {
            noInlineConfig: true,
            reportUnusedDisableDirectives: "error",
        },
        languageOptions: {
            ecmaVersion: 2022,
            globals: globals.browser,
            parserOptions: {
                // vite.config.ts belongs to tsconfig.node.json, which no tsconfig.json
                // references by directory, so the service opens it through that file.
                projectService: {
                    allowDefaultProject: ["vite.config.ts"],
                    defaultProject: "tsconfig.node.json",
                },
                tsconfigRootDir: import.meta.dirname,
            },
        },
        rules: {
            "@typescript-eslint/no-explicit-any": "error",
            "@typescript-eslint/no-non-null-assertion": "error",
            "@typescript-eslint/no-unused-vars": [
                "error",
                { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
            ],
            "@typescript-eslint/no-floating-promises": "error",
            "@typescript-eslint/no-misused-promises": [
                "error",
                { checksVoidReturn: { attributes: false } },
            ],
            "@typescript-eslint/consistent-type-imports": [
                "error",
                { prefer: "type-imports", fixStyle: "separate-type-imports" },
            ],
            "@typescript-eslint/restrict-template-expressions": ["error", { allowNumber: true }],

            // `x as T` silently overrides the checker. Only `as const` survives, because it
            // narrows a literal and cannot lie about a value's shape. Narrow with a type guard
            // or `satisfies` instead.
            "no-restricted-syntax": [
                "error",
                {
                    selector:
                        "TSAsExpression:not([typeAnnotation.type='TSTypeReference'][typeAnnotation.typeName.name='const'])",
                    message:
                        "Do not use `as` type assertions (only `as const`). Narrow with a type guard or use `satisfies`.",
                },
                {
                    selector: "TSTypeAssertion",
                    message:
                        "Do not use `<T>value` type assertions. Narrow with a type guard or use `satisfies`.",
                },
            ],

            "no-console": ["error", { allow: ["warn", "error"] }],
            "prefer-const": "error",
            "no-var": "error",
            eqeqeq: ["error", "always", { null: "ignore" }],
        },
    },
    {
        files: ["vite.config.ts"],
        languageOptions: {
            globals: globals.node,
        },
    },
);
