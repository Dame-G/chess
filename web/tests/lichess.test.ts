import { describe, expect, it } from "vitest";
import { clockLabel, declineText, oursOf, speedOf } from "../src/lichess";

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
});
