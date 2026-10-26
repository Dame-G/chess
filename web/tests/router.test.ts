import { describe, expect, it } from "vitest";
import type { RouteLocationNormalizedLoaded, RouteLocationRaw } from "vue-router";
import { router } from "../src/router";

/** Where an address ends up, following redirects without loading any view. */
function land(path: string): string {
  let to = router.resolve(path);
  for (let hops = 0; hops < 5; hops++) {
    const redirect = to.matched.at(-1)?.redirect;
    if (!redirect) return to.fullPath;
    const loaded = to as unknown as RouteLocationNormalizedLoaded;
    const next = typeof redirect === "function" ? redirect(loaded, loaded) : redirect;
    to = router.resolve(next as RouteLocationRaw);
  }
  throw new Error(`${path} redirects in a loop`);
}

describe("old addresses", () => {
  it.each([
    ["/analysis?fen=x", "/board?fen=x"],
    ["/lab?new=1", "/improve?new=1"],
    ["/lab/3?stage=bench", "/improve/experiments/3?stage=bench"],
    ["/experiments?q=nmp", "/improve?q=nmp"],
    ["/experiments/3", "/improve/experiments/3"],
    ["/tune?new=1", "/improve?new=tune"],
    ["/tune", "/improve"],
    ["/tune/2", "/improve/tunes/2"],
    ["/engines", "/versions"],
    ["/engines/4", "/versions/4"],
    ["/tests/2/games/5", "/games/2/5"],
    ["/runner", "/"],
    ["/nowhere/at/all", "/"],
  ])("%s goes to %s", (from, to) => {
    expect(land(from)).toBe(to);
  });

  it("leaves current addresses alone", () => {
    expect(land("/versions/spsa%2Ftunables")).toBe("/versions/spsa%2Ftunables");
    expect(land("/matches/7")).toBe("/matches/7");
    expect(land("/lichess/compare?players=me,Magnus")).toBe("/lichess/compare?players=me,Magnus");
  });
});
