/**
 * Fixture states for Settings. `?fixtures=settings-edge`: no analysis engine and no Reckless, with long paths.
 * `?fixtures=settings-error`: the machine's health and the token fail to load.
 */
import { error, reply } from "./fixtures-http";

const LONG_DIR =
  "C:\\Users\\a-person-with-a-long-name\\AppData\\Local\\Programs\\chess-with-a-deeply\\nested\\data\\folder";

/** The answer for a route in a Settings state, or undefined to leave it to the usual fixtures. */
export function settingsRoute(mode: string | null, path: string): Response | undefined {
  if (mode === "settings-error") {
    if (path === "/api/diagnostics") return error(500, "internal", "The diagnostics did not run.");
    if (path === "/api/token") return error(500, "internal", "The token could not be read.");
  }
  if (mode === "settings-edge") {
    if (path === "/api/diagnostics")
      return reply("/api/diagnostics", "get", {
        ok: false,
        items: [
          {
            key: "analysis",
            level: "fail",
            ok: false,
            title: "Analysis engine",
            detail: `Stockfish was not found in ${LONG_DIR}\\tools\\stockfish-windows-x86-64-avx2`,
            action: { label: "Set it up", command: "chess setup" },
          },
          {
            key: "reckless",
            level: "info",
            ok: true,
            title: "Reckless",
            detail: "Reckless is not installed: another engine to play, with its own levels, and to analyse with",
            action: { label: "Set it up", command: "chess setup --only reckless" },
          },
        ],
      });
  }
  return undefined;
}
