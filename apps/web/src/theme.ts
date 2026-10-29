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

/**
 * eos follows the app's scales, not the other way round (docs/ui-brief.md, section 2): its gaps and spaces sit on the
 * 4 px grid, its body text is 14 px, its controls are 32 px with a fine pointer and 44 px with a coarse one, and it has
 * two radii. This comes after the theme's own tokens, in the same unlayered sheet, so it wins on order: eos's own
 * stylesheet is in layers and the theme's tokens are not.
 */
const eosOverrides = `
:root:root {
  --e-space-1: var(--sp-1);
  --e-space-2: var(--sp-2);
  --e-space-3: var(--sp-3);
  --e-space-4: var(--sp-4);
  --e-space-5: 1.25rem;
  --e-space-6: var(--sp-5);
  --e-space-8: var(--sp-6);
  --e-space-10: 2.5rem;
  --e-space-12: 3rem;
  --e-space-16: 4rem;
  --e-pad-panel: var(--sp-4);
  --e-pad-control: var(--sp-3);
  --e-pad-control-sm: var(--sp-2);
  --e-pad-chip: var(--sp-2);
  --e-pad-card: var(--sp-3);
  --e-pad-cell: var(--sp-1) var(--sp-2);
  --e-text-sm: 0.875rem;
  --e-text-md: 0.875rem;
  --e-type-helper: var(--e-weight-regular) var(--e-text-xs) / 1.35 var(--e-font-sans);
  --e-radius-sm: var(--e-radius-md);
  --e-depth: 0px;
  --e-depth-thin: 0px;
  --e-depth-bold: 1px;
  --e-control-height: 2rem;
  --e-tap: 2rem;
}

@media (pointer: coarse) {
  :root:root {
    --e-control-height: 2.75rem;
    --e-tap: 2.75rem;
  }
}
`;

/** The two schemes as fixed themes. The page picks one with `data-scheme` on `<html>` (see `scheme.ts`), not by media query. */
export const saddleLight = defineTheme({ ...saddle, scheme: "light", dark: undefined });
export const saddleDark = defineTheme({ ...darkVariant(saddle), name: "saddle-dark" });

/** Both as CSS: light on `:root`, dark under `:root[data-scheme="dark"]`. Unlayered, so dark wins on specificity. */
export const themeCss = `${saddleLight.css({ layer: false })}\n${saddleDark.css({ selector: ':root[data-scheme="dark"]', layer: false })}\n${eosOverrides}`;
