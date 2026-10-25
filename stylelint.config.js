export default {
  extends: ["stylelint-config-standard", "stylelint-config-html"],
  ignoreFiles: ["**/dist/**", "**/node_modules/**"],
  rules: {
    "selector-class-pattern": null,
    "custom-property-pattern": null,
    "keyframes-name-pattern": null,
    "declaration-no-important": true,
    "at-rule-no-unknown": [true, { ignoreAtRules: ["layer"] }],
    "no-descending-specificity": null,
    "value-keyword-case": null,
    "color-function-notation": null,
    "alpha-value-notation": null,
    "media-feature-range-notation": null,
  },
};
