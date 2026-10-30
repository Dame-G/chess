import { describe, expect, it } from "vitest";
import type { Drill, DrillAnswerResult, PrepLine, ThemeStat } from "../src/api/types";
import { hintSquare, pips, scopeQuery, scopeTitle, streak, summarise } from "../src/board/drill";
import { themeText, trainable } from "../src/board/themes";
import { byNeed, earlyText, flags, scoreText } from "../src/sheets/prep";
import { lost, namedBy, shareText, totals } from "../src/sheets/themes";

const drill = (id: number, kind: Drill["theme"]["kind"] = "other", interval = 1): Drill =>
  ({ id, theme: { opening: null, phase: "opening", kind, detail: null }, interval_days: interval }) as Drill;
const answer = (
  id: number,
  outcome: "pass" | "fail",
  hinted = false,
  kind: Drill["theme"]["kind"] = "other",
  interval = 1,
) => ({ outcome, hinted, drill: drill(id, kind, interval) }) as DrillAnswerResult;

describe("a sitting", () => {
  it("shows what came of each drill, and the run of clean passes", () => {
    const queue = [drill(1), drill(2), drill(3), drill(4)];
    const results = new Map([
      [1, answer(1, "pass")],
      [2, answer(2, "pass", true)],
      [3, answer(3, "fail")],
    ]);
    expect(pips(queue, results, 4)).toEqual(["pass", "half", "fail", "now"]);
    expect(pips(queue, results, null)[3]).toBe("todo");
    expect(streak([1, 2, 3], results)).toBe(0);
    expect(streak([3, 1], new Map([...results, [1, answer(1, "pass")]]))).toBe(1);
    expect(streak([], results)).toBe(0);
  });

  it("adds up the hit rate with a hint counting half, by theme, worst first", () => {
    const s = summarise([
      answer(1, "pass", false, "hanging_piece", 6),
      answer(2, "pass", true, "hanging_piece", 1),
      answer(3, "fail", false, "missed_tactic", 1),
    ]);
    expect([s.total, s.clean, s.hinted, s.missed]).toEqual([3, 1, 1, 1]);
    expect(s.rate).toBeCloseTo(0.5);
    expect(s.themes.map((t) => [t.kind, t.credit, t.total, t.back])).toEqual([
      ["missed_tactic", 0, 1, 1],
      ["hanging_piece", 1.5, 2, 1],
    ]);
    expect(summarise([]).rate).toBe(0);
  });

  it("asks the server for a theme or a line, and names it", () => {
    expect(scopeQuery({ type: "day" })).toEqual({});
    expect(scopeQuery({ type: "theme", theme: "hanging_piece" })).toEqual({ theme: "hanging_piece" });
    expect(scopeQuery({ type: "line", name: "Italian Game", games: [4, 9], maxPly: 24 })).toEqual({
      games: "4,9",
      max_ply: 24,
    });
    expect(scopeTitle({ type: "theme", theme: "missed_tactic" })).toBe("Missed tactics");
    expect(scopeTitle({ type: "line", name: "Italian Game", games: [], maxPly: 24 })).toBe("Italian Game");
  });

  it("marks the piece only from the second hint", () => {
    expect(hintSquare([{ level: 1 }])).toBeNull();
    expect(hintSquare([{ level: 1 }, { level: 2, from: "f1" }])).toBe("f1");
  });
});

describe("themes", () => {
  const stat = (patch: Partial<ThemeStat>): ThemeStat => ({
    kind: "hanging_piece",
    count: 3,
    games: 2,
    swing: 30,
    per_game: 1.5,
    share: 0.4,
    causes: [{ name: "a knight", count: 2, swing: 20 }],
    openings: [{ name: "Italian Game", count: 2, swing: 20 }],
    examples: [],
    ...patch,
  });

  it("writes costs and shares", () => {
    expect(lost(2.44)).toBe("−2.4%");
    expect(lost(0)).toBe("0%");
    expect(shareText(0.004)).toBe("<1%");
    expect(shareText(0.346)).toBe("35%");
    expect(totals([stat({}), stat({ count: 1, per_game: 0.5 })])).toMatchObject({ count: 4, perGame: 2 });
  });

  it("names the opening for opening errors and the cause for the rest", () => {
    expect(namedBy(stat({ kind: "opening_error" }))?.label).toBe("Mostly in");
    expect(namedBy(stat({}))?.label).toBe("Most often");
    expect(namedBy(stat({ causes: [] }))).toBeNull();
  });

  it("words a drill's theme and drills every theme but other", () => {
    expect(themeText("hanging_piece", "a knight")).toBe("Hanging piece · a knight");
    expect(themeText("time_trouble")).toBe("Time trouble");
    expect(trainable("other")).toBe(false);
    expect(trainable("king_safety")).toBe(true);
  });
});

describe("prep", () => {
  const line = (patch: Partial<PrepLine>): PrepLine =>
    ({
      games: 5,
      score: 0.6,
      reviewed: 4,
      early_games: 1,
      scores_badly: false,
      goes_wrong_early: false,
      ...patch,
    }) as PrepLine;

  it("puts the lines that need work first, then the most played", () => {
    const lines = [line({ games: 9 }), line({ games: 4, scores_badly: true }), line({ games: 6 })];
    expect(byNeed(lines).map((l) => l.games)).toEqual([4, 9, 6]);
  });

  it("says why a line is flagged and how it goes", () => {
    expect(flags(line({ scores_badly: true, goes_wrong_early: true }))).toEqual(["Scores badly", "Goes wrong early"]);
    expect(flags(line({}))).toEqual([]);
    expect(scoreText(line({ score: null }))).toBe("–");
    expect(scoreText(line({ score: 0.625 }))).toBe("63%");
    expect(earlyText(line({ reviewed: 0 }))).toBe("not reviewed");
    expect(earlyText(line({}))).toBe("1 of 4 games");
  });
});
