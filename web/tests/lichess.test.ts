import { describe, expect, it } from "vitest";
import {
  challengeLine,
  clockAllowed,
  clockLabel,
  declineText,
  liveGame,
  meClockOptions,
  oursOf,
  personalTokenUrl,
  ratingList,
  speedOf,
} from "../src/lichess";

describe("lichess text", () => {
  it("writes clocks in minutes like Lichess", () => {
    expect(clockLabel(180, 2)).toBe("3+2");
    expect(clockLabel(30, 0)).toBe("½+0");
    expect(clockLabel(90, 1)).toBe("1.5+1");
  });

  it("picks the speed from the expected length", () => {
    expect(speedOf(60, 0)).toBe("bullet");
    expect(speedOf(180, 2)).toBe("blitz");
    expect(speedOf(600, 5)).toBe("rapid");
    expect(speedOf(1800, 0)).toBe("classical");
  });

  it("explains why a challenge was declined", () => {
    expect(declineText("tooFast", true)).toBe("too fast");
    expect(declineText("casual", true)).toBe("rated games only");
    expect(declineText("generic", true)).toBe("no engine chosen");
    expect(declineText(null, true)).toBe("");
  });

  it("reads a game from our side", () => {
    const game = (result: "1-0" | "0-1" | "1/2-1/2" | "*", status: "running" | "finished", side: "white" | "black") =>
      oursOf({
        white: { name: "a", commit: "" },
        black: { name: "b", commit: "" },
        lichess: { id: "x", url: "", rated: false, speed: "blitz", our_side: side },
        result,
        status,
      });
    expect(game("1-0", "finished", "white")).toBe("won");
    expect(game("1-0", "finished", "black")).toBe("lost");
    expect(game("1/2-1/2", "finished", "black")).toBe("drew");
    expect(game("*", "running", "black")).toBe("live");
  });

  it("offers only clocks the Board API allows", () => {
    expect(meClockOptions("seek").every((c) => clockAllowed("seek", c.value))).toBe(true);
    expect(meClockOptions("challenge").every((c) => clockAllowed("challenge", c.value))).toBe(true);
    // 5+3 is 7 minutes estimated: blitz, so a challenge but not a seek.
    expect(clockAllowed("seek", "5+3")).toBe(false);
    expect(clockAllowed("challenge", "5+3")).toBe(true);
    expect(clockAllowed("challenge", "1+0")).toBe(false);
    expect(clockAllowed("seek", "10+0")).toBe(true);
  });

  it("links to a token form with the scopes ticked", () => {
    expect(personalTokenUrl).toContain("scopes[]=board:play");
    expect(personalTokenUrl).toContain("scopes[]=challenge:read");
    expect(personalTokenUrl).toContain("scopes[]=challenge:write");
  });

  it("lists the ratings a person has and writes a challenge", () => {
    expect(ratingList({ blitz: 1600, bullet: 1500, rapid: null })).toEqual([
      { speed: "bullet", rating: 1500 },
      { speed: "blitz", rating: 1600 },
    ]);
    expect(challengeLine({ clock: { initial: 300, increment: 3 }, rated: true })).toBe("5+3 rated");
    expect(challengeLine({ clock: null, days: 3, rated: false })).toBe("3 days casual");
  });

  it("finds the game being played, here or on lichess.org", () => {
    const game = {
      id: 4,
      white: { name: "me", commit: "" },
      black: { name: "Alpha", commit: "" },
      result: "*",
      status: "running",
      lichess: { our_side: "white" },
    };
    expect(liveGame({ playing: [4], elsewhere: [] }, [game as never])).toEqual({ id: 4, opponent: "Alpha" });
    expect(liveGame({ playing: [], elsewhere: [{ id: "x", speed: "bullet", url: "u" }] }, [])).toEqual({ url: "u" });
    expect(liveGame({ playing: [], elsewhere: [] }, [])).toBeUndefined();
  });
});
