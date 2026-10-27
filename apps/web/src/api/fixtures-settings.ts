/**
 * Fixture states for Settings and its dialogs. `?fixtures=settings-edge`: tools missing, long paths, a harness that was
 * never checked. `?fixtures=settings-error`: the machine's health, harness and token fail to load.
 */
import { error, reply } from "./fixtures-http";

const LONG_DIR =
  "C:\\Users\\a-person-with-a-long-name\\AppData\\Local\\Programs\\dameless-with-a-deeply\\nested\\data\\folder\\reckless";

const NEVER_CHECKED = {
  checked: false,
  current: false,
  trusted: false,
  passed: null,
  at: null,
  idea: null,
  running: null,
  last: null,
  summary: "No check has run on this machine yet, so every trial verdict here says it is not trusted.",
};

/** The answer for a route in a Settings state, or undefined to leave it to the usual fixtures. */
export function settingsRoute(mode: string | null, path: string): Response | undefined {
  if (mode === "settings-error") {
    if (path === "/api/diagnostics") return error(500, "internal", "The diagnostics did not run.");
    if (path === "/api/harness") return error(500, "internal", "The harness could not be read.");
    if (path === "/api/token") return error(500, "internal", "The token could not be read.");
  }
  if (mode === "settings-edge") {
    if (path === "/api/harness") return reply("/api/harness", "get", NEVER_CHECKED);
    if (path === "/api/diagnostics")
      return reply("/api/diagnostics", "get", {
        ok: false,
        items: [
          {
            key: "fastchess",
            level: "fail",
            ok: false,
            title: "fastchess",
            detail: `fastchess was not found in ${LONG_DIR}\\tools\\fastchess-v1.8.2-windows-x86-64`,
            action: { label: "Install it", command: "dameless setup" },
          },
          {
            key: "book",
            level: "fail",
            ok: false,
            title: "Opening book",
            detail: "The opening book is not installed.",
            action: { label: "Install it", command: "dameless setup" },
          },
          {
            key: "engine",
            level: "ok",
            ok: true,
            title: "Engine checkout",
            detail: `${LONG_DIR}, on main.`,
          },
          {
            key: "clang",
            level: "warn",
            ok: true,
            title: "clang",
            detail: "clang is not on the PATH, so builds with Syzygy support cannot be made.",
            action: { label: "Install it", command: "winget install LLVM.LLVM --accept-package-agreements --silent" },
          },
          { key: "cuda", level: "info", ok: true, title: "CUDA", detail: "CUDA is not installed. Training is off." },
        ],
      });
  }
  return undefined;
}
