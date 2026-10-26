import { describe, expect, it } from "vitest";
import {
  clockText,
  matchTimeText,
  outcomeText,
  sideScore,
  youOutcome,
  youResult,
  youSide,
} from "../src/board/matchText";

describe("match text", () => {
  it("writes clocks as minutes and seconds, with tenths when low", () => {
    expect(clockText(600_000)).toBe("10:00");
    expect(clockText(61_000)).toBe("1:01");
    expect(clockText(9_950)).toBe("0:09.9");
    expect(clockText(400)).toBe("0:00.4");
    expect(clockText(-50)).toBe("0:00.0");
    expect(clockText(3_725_000)).toBe("1:02:05");
  });

  it("says who won and why in plain words", () => {
    const ended = (result: "1-0" | "0-1" | "1/2-1/2" | "*", reason: Parameters<typeof outcomeText>[0]["reason"]) =>
      outcomeText({ result, reason, status: "finished" });
    expect(ended("0-1", "time")).toBe("Black won on time");
    expect(ended("1-0", "checkmate")).toBe("White won by checkmate");
    expect(ended("1/2-1/2", "repetition")).toBe("Draw by threefold repetition");
    expect(ended("1/2-1/2", "fifty_moves")).toBe("Draw by the fifty-move rule");
    expect(outcomeText({ result: "*", reason: "aborted", status: "aborted" })).toBe("Stopped before the end");
  });

  it("words the endings of Lichess games", () => {
    const ended = (result: "1-0" | "0-1" | "1/2-1/2" | "*", reason: Parameters<typeof outcomeText>[0]["reason"]) =>
      outcomeText({ result, reason, status: "finished" });
    expect(ended("1-0", "resigned")).toBe("Black resigned");
    expect(ended("0-1", "resigned")).toBe("White resigned");
    expect(ended("1/2-1/2", "agreement")).toBe("Draw agreed");
    expect(ended("0-1", "abandoned")).toBe("White left the game");
    expect(ended("*", "no_start")).toBe("Aborted, the game never started");
    expect(ended("1-0", "cheat")).toBe("White won, Black was disqualified");
    expect(outcomeText({ result: "*", reason: "aborted", status: "aborted", lichess: null })).toBe(
      "Stopped before the end",
    );
    const lichess = { id: "x", url: "", rated: true, speed: "blitz", our_side: "white" } as const;
    expect(outcomeText({ result: "*", reason: "aborted", status: "aborted", lichess })).toBe("Aborted");
  });

  it("describes time controls and scores", () => {
    expect(matchTimeText({ type: "clock", seconds: 10, increment: 0.1 })).toBe("10+0.1");
    expect(matchTimeText({ type: "nodes", nodes: 25_000 })).toBe("25k nodes");
    expect(sideScore("1-0", "white")).toBe("1");
    expect(sideScore("1-0", "black")).toBe("0");
    expect(sideScore("1/2-1/2", "black")).toBe("½");
  });

  it("tells a game you played in the second person", () => {
    const you = { human: true, name: "You", commit: "" };
    const engine = { build: 2, name: "main", commit: "abc" };
    const as = (white: typeof you | typeof engine, black: typeof you | typeof engine) => ({ white, black });
    const asWhite = as(you, engine);
    expect(youSide(asWhite)).toBe("white");
    expect(youSide(as(engine, you))).toBe("black");
    const done = (result: "1-0" | "0-1" | "1/2-1/2", reason: "time" | "resigned" | "checkmate" | "agreement") => ({
      ...asWhite,
      result,
      reason,
      status: "finished" as const,
    });
    expect(youOutcome(done("0-1", "time"))).toBe("You lost on time");
    expect(youOutcome(done("1-0", "resigned"))).toBe("Your opponent resigned");
    expect(youOutcome(done("0-1", "resigned"))).toBe("You resigned");
    expect(youOutcome(done("1-0", "checkmate"))).toBe("You won by checkmate");
    expect(youOutcome(done("1/2-1/2", "agreement"))).toBe("Draw agreed");
    expect(youResult(done("1-0", "time"))).toBe("Won");
    expect(youResult(done("1/2-1/2", "agreement"))).toBe("Draw");
  });
});
