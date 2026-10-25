import { darkVariant, defineTheme, type ThemeInput } from "@eos-ui/tokens";

/**
 * "Saddle": cognac leather and brass on linen by day, on espresso and walnut by night. Sharp corners, a thin slab and
 * snappy motion: a dense workbench, not a brochure. eos derives every other colour (the deep primary, soft tints,
 * borders, muted text) from these, holding its contrast minimums.
 */
const saddle: ThemeInput = {
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

/** The two schemes as fixed themes. The page picks one with `data-scheme` on `<html>` (see `scheme.ts`), not by media query. */
export const saddleLight = defineTheme({ ...saddle, scheme: "light", dark: undefined });
export const saddleDark = defineTheme({ ...darkVariant(saddle), name: "saddle-dark" });

/** Both as CSS: light on `:root`, dark under `:root[data-scheme="dark"]`. Unlayered, so dark wins on specificity. */
export const themeCss = `${saddleLight.css({ layer: false })}\n${saddleDark.css({ selector: ':root[data-scheme="dark"]', layer: false })}`;
