import type { ThemeInput } from "@eos-ui/tokens";

/**
 * "Saddle": cognac leather and brass on linen by day, on espresso and walnut by night. Sharp corners, a thin slab and
 * snappy motion: a dense workbench, not a brochure. eos derives every other colour (the deep primary, soft tints,
 * borders, muted text) from these, holding its contrast minimums.
 */
export const saddle: ThemeInput = {
  name: "saddle",
  brand: "#b5652a",
  accent: "#a8842c",
  background: "#efe6d8",
  surface: "#faf5ec",
  foreground: "#2b1e16",
  success: "#4f7d3a",
  warning: "#c4861a",
  danger: "#a1302a",
  fonts: {
    sans: "'Hanken Grotesk Variable', 'Avenir Next', system-ui, sans-serif",
    mono: "'JetBrains Mono Variable', ui-monospace, Consolas, monospace",
  },
  radius: 3,
  depth: 1,
  motion: "snappy",
  scheme: "auto",
  density: "compact",
  dark: {
    background: "#14100d",
    surface: "#1e1814",
    foreground: "#efe4d4",
    brand: "#d4884a",
  },
};
