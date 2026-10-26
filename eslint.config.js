import js from "@eslint/js";
import tseslint from "typescript-eslint";
import vue from "eslint-plugin-vue";
import a11y from "eslint-plugin-vuejs-accessibility";
import globals from "globals";

// The same rules as the eos library, so code reads the same in both repositories.
export default tseslint.config(
  { ignores: ["**/dist/**", "**/node_modules/**", "**/target*/**", "apps/web/src/api/schema.d.ts", "**/.claude/**"] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...vue.configs["flat/recommended"],
  ...a11y.configs["flat/recommended"],
  {
    languageOptions: { globals: { ...globals.browser, ...globals.node } },
  },
  {
    files: ["**/*.vue"],
    languageOptions: { parserOptions: { parser: tseslint.parser } },
  },
  {
    rules: {
      "no-empty": ["error", { allowEmptyCatch: true }],
      "vue/multi-word-component-names": "off",
      "vue/block-order": ["error", { order: ["script", "template", "style"] }],
      "vue/define-macros-order": ["error", { order: ["defineOptions", "defineProps", "defineEmits", "defineSlots"] }],
      "vue/require-default-prop": "off",
      "vue/html-self-closing": "off",
      "vue/max-attributes-per-line": "off",
      "vue/singleline-html-element-content-newline": "off",
      "vue/html-indent": "off",
      "vue/html-closing-bracket-newline": "off",
      "@typescript-eslint/no-unused-vars": ["error", { argsIgnorePattern: "^_" }],
    },
  },
);
