const KEY = "chess.fixtures";

/**
 * Whether this tab runs on fixtures, and which: `?fixtures` (or `?fixtures=fairplay`) in the address turns them on for
 * the tab, `?fixtures=off` turns them off again, and `VITE_FIXTURES=1` at build time turns them on everywhere. The
 * choice is kept for the tab, so moving between the board and the sheets keeps it.
 */
export function fixtureMode(): string | null {
  const asked = new URLSearchParams(location.search).get("fixtures");
  try {
    if (asked === "off") sessionStorage.removeItem(KEY);
    else if (asked !== null) sessionStorage.setItem(KEY, asked || "on");
    const kept = sessionStorage.getItem(KEY);
    if (kept) return kept;
  } catch {
    if (asked !== null && asked !== "off") return asked || "on";
  }
  return import.meta.env["VITE_FIXTURES"] === "1" ? "on" : null;
}
