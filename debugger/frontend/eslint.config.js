import js from "@eslint/js";
import globals from "globals";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import tseslint from "typescript-eslint";
import eslintConfigPrettier from "eslint-config-prettier";

export default tseslint.config(
  { ignores: ["dist"] },
  {
    files: ["**/*.{ts,tsx}"],
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    languageOptions: {
      ecmaVersion: 2020,
      globals: globals.browser,
    },
    plugins: {
      "react-hooks": reactHooks,
      "react-refresh": reactRefresh,
    },
    rules: {
      // Classic hook rules only. eslint-plugin-react-hooks v7's "recommended"
      // config also bundles React Compiler readiness rules (refs,
      // immutability, purity, set-state-in-effect, etc.) that flag idiomatic
      // patterns this codebase uses intentionally (e.g. the "latest ref"
      // pattern). Adopting those is a separate decision from CI lint
      // enforcement, so they're deliberately left out here.
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "warn",
      // "warn" rather than the plugin's default "error": several files here
      // intentionally co-export a provider component with its hook (the
      // standard React context pattern) or a component with small helper
      // constants/functions, which the rule otherwise flags. That trade-off
      // (giving up some Fast Refresh granularity in dev) is a deliberate,
      // pre-existing choice, not something to force-fix for a lint PR.
      "react-refresh/only-export-components": ["warn", { allowConstantExport: true }],
      "@typescript-eslint/no-unused-vars": [
        "warn",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
    },
  },
  eslintConfigPrettier,
);
