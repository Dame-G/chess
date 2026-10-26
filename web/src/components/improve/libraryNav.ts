/** Where the arrow keys take the chosen row: one step along the list, clamped; null when the keys are for something else. */
export function stepTarget<T>(list: readonly T[], at: number, delta: number, event: KeyboardEvent): T | null {
  const target = event.target instanceof Element ? event.target : null;
  if (target?.closest("input, textarea, select, [role=radio], [role=combobox], [role=listbox], [role=dialog]"))
    return null;
  if (!list.length) return null;
  event.preventDefault();
  return list[Math.min(list.length - 1, Math.max(0, at < 0 ? 0 : at + delta))]!;
}

/** Below 75rem the library sits under the stage: this brings it into view. */
export function toLibrary(label: string) {
  const calm = matchMedia("(prefers-reduced-motion: reduce)").matches;
  document
    .querySelector(`aside[aria-label='${label}']`)
    ?.scrollIntoView({ behavior: calm ? "auto" : "smooth", block: "start" });
}
