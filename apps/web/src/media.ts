import { onScopeDispose, ref, type Ref } from "vue";

/**
 * The one list of breakpoints (docs/ui-brief.md, 4.1). Code names a breakpoint instead of writing its number, and a
 * stylesheet, which cannot read a constant in a media query, writes the same number with the name in a comment:
 * `sm` 30rem, `md` 48rem, `lg` 75rem, `xl` 100rem.
 */
export const BREAKPOINTS = { sm: "30rem", md: "48rem", lg: "75rem", xl: "100rem" } as const;
export type Breakpoint = keyof typeof BREAKPOINTS;

/** The query for a window at least this wide: `up("lg")` is `(width >= 75rem)`. */
export const up = (name: Breakpoint): string => `(width >= ${BREAKPOINTS[name]})`;

/** The query for a window narrower than this: `down("md")` is `(width < 48rem)`. */
export const down = (name: Breakpoint): string => `(width < ${BREAKPOINTS[name]})`;

/** Whether a media query matches now, kept up to date until the calling scope ends. */
export function useMedia(query: string): Ref<boolean> {
  const list = typeof matchMedia === "function" ? matchMedia(query) : undefined;
  const matches = ref(list?.matches ?? false);
  if (list) {
    const on = (e: MediaQueryListEvent) => (matches.value = e.matches);
    list.addEventListener("change", on);
    onScopeDispose(() => list.removeEventListener("change", on));
  }
  return matches;
}
