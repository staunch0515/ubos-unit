// Lint gate of UBS-ARC-07 §4 for the TypeScript SDK.
import js from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist/", "build/", "node_modules/"] },
  js.configs.recommended,
  ...tseslint.configs.strict,
  {
    files: ["tools/**/*.mjs", "examples/**/*.mjs"],
    languageOptions: {
      globals: { process: "readonly", console: "readonly", URL: "readonly", URLSearchParams: "readonly", window: "readonly" },
    },
  },
);
