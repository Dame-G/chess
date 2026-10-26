import { describe, expect, it } from "vitest";
import type { PlayerStats } from "../src/api/queries";
import {
  buildSheet,
  momentPath,
  moveLabel,
  readState,
  reviewCost,
  sampleOf,
  stateQuery,
} from "../src/components/compare/compare";

const wdl = (wins: number, draws: number, losses: number) => {
  const games = wins + draws + losses;
  return { games, wins, draws, losses, score: games ? ((wins + draws / 2) / games) * 100 : null };
};
const zero = {
  brilliant: 0,
  great: 0,
  best: 0,
  excellent: 0,
  good: 0,
  book: 0,
  inaccuracy: 0,
  mistake: 0,
  blunder: 0,
  miss: 0,
};

/** A column of the comparison, with only what a test says. */
function player(over: Partial<PlayerStats> & { player: string }): PlayerStats {
  return {
    username: over.player,
    games: 50,
    reviewed: 50,
    pending_review: 0,
    results: { overall: wdl(25, 0, 25), white: wdl(12, 0, 13), black: wdl(13, 0, 12) },
    accuracy: { overall: 80, opening: 85, middlegame: 78, endgame: 76 },
    acpl: 30,
    by_speed: [{ speed: "blitz", games: 50, reviewed: 50, results: wdl(25, 0, 25), accuracy: 80, acpl: 30 }],
    counts: { ...zero, best: 100, blunder: 5 },
    per_game: { ...zero, best: 2, blunder: 0.1 },
    judgements: { inaccuracy: 0, mistake: 0, blunder: 5 },
    opponents: { average_rating: 1500, performance: 1500 },
    openings: [{ name: "Sicilian Defense", results: wdl(5, 0, 5) }],
    time: {
      games: 50,
      average_ms: 4000,
      opening_ms: 2000,
      middlegame_ms: 5000,
      endgame_ms: 3000,
      time_trouble_blunders: 1,
      time_trouble_blunders_per_game: 0.02,
    },
    longest_win_streak: 4,
    notable: { brilliant: [], blunders: [], most_accurate_game: null, least_accurate_game: null },
    ...over,
  } as PlayerStats;
}

const row = (sheet: ReturnType<typeof buildSheet>, id: string) =>
  sheet.groups.flatMap((g) => g.rows).find((r) => r.id === id)!;

describe("the address", () => {
  it("always starts with you, once", () => {
    expect(readState({}).players).toEqual(["me"]);
    expect(readState({ players: "Magnus,me,magnus,bot" }).players).toEqual(["me", "Magnus", "bot"]);
  });

  it("drops what is not a Lichess username", () => {
    expect(readState({ players: "ok_name,../etc,a,x y" }).players).toEqual(["me", "ok_name"]);
  });

  it("reads the filters, and falls back to the defaults", () => {
    expect(readState({ speeds: "rapid,blitz,nonsense", rated: "1", limit: "100" })).toMatchObject({
      speeds: ["blitz", "rapid"],
      rated: true,
      limit: 100,
    });
    expect(readState({ limit: "7" }).limit).toBe(50);
    expect(readState({ speeds: "bullet,blitz,rapid,classical" }).speeds).toEqual([]);
  });

  it("writes only what is not the default, and reads back the same", () => {
    expect(stateQuery(readState({}))).toEqual({
      players: undefined,
      speeds: undefined,
      rated: undefined,
      limit: undefined,
    });
    const q = stateQuery({ players: ["me", "Magnus"], speeds: ["blitz"], rated: true, limit: 20 });
    expect(q).toEqual({ players: "me,Magnus", speeds: "blitz", rated: "1", limit: "20" });
    expect(stateQuery(readState(q))).toEqual(q);
  });
});

describe("the sheet", () => {
  const a = player({ player: "me", accuracy: { overall: 80, opening: 85, middlegame: 78, endgame: 70 }, acpl: 30 });
  const b = player({ player: "Magnus", accuracy: { overall: 91, opening: 85, middlegame: 90, endgame: 95 }, acpl: 12 });
  const sheet = buildSheet([a, b]);

  it("marks the better value of a row: higher accuracy, lower centipawn loss", () => {
    expect(row(sheet, "acc").cells.map((c) => !!c.lead)).toEqual([false, true]);
    expect(row(sheet, "acpl").cells.map((c) => !!c.lead)).toEqual([false, true]);
  });

  it("leaves a tie unmarked", () => {
    expect(row(sheet, "acc-opening").cells.map((c) => !!c.lead)).toEqual([false, false]);
  });

  it("counts the rows each column leads", () => {
    expect(sheet.contested).toBeGreaterThan(3);
    expect(sheet.leads[1]).toBeGreaterThan(sheet.leads[0]!);
    expect(sheet.leads[0]! + sheet.leads[1]!).toBeLessThanOrEqual(sheet.contested);
  });

  it("gives no lead to a column with almost nothing reviewed, but still shows its numbers", () => {
    const few = buildSheet([a, player({ player: "x", reviewed: 2, accuracy: { overall: 99 } })]);
    expect(row(few, "acc").cells.map((c) => !!c.lead)).toEqual([false, false]);
    expect(row(few, "acc").cells[1]!.text).toBe("99.0%");
  });

  it("shows a dash where nothing is reviewed, and never a lead", () => {
    const none = buildSheet([a, player({ player: "x", reviewed: 0 })]);
    expect(row(none, "acc").cells[1]).toMatchObject({ text: "–", value: null });
    expect(row(none, "acc").cells.some((c) => c.lead)).toBe(false);
  });

  it("measures results over every game, reviewed or not", () => {
    const results = { overall: wdl(40, 0, 10), white: wdl(20, 0, 5), black: wdl(20, 0, 5) };
    const all = buildSheet([a, player({ player: "x", reviewed: 0, results })]);
    expect(row(all, "res").cells.map((c) => !!c.lead)).toEqual([false, true]);
    expect(row(all, "res").cells[1]!.sub).toBe("40–0–10");
  });

  it("lists a speed only when someone has reviewed games at it", () => {
    expect(sheet.groups[0]!.rows.map((r) => r.id)).toContain("acc-blitz");
    expect(sheet.groups[0]!.rows.map((r) => r.id)).not.toContain("acc-rapid");
  });

  it("shows the book row only when someone has a book move", () => {
    expect(sheet.groups.flatMap((g) => g.rows).some((r) => r.id === "q-book")).toBe(false);
    const book = buildSheet([player({ player: "me", counts: { ...zero, book: 4 } })]);
    expect(book.groups.flatMap((g) => g.rows).some((r) => r.id === "q-book")).toBe(true);
  });

  it("builds the quality bar from the counts, and omits it when there are none", () => {
    const mix = row(sheet, "mix").cells[0]!.mix!;
    expect(mix.reduce((n, m) => n + m.share, 0)).toBeCloseTo(1);
    expect(row(buildSheet([player({ player: "me", counts: zero })]), "mix").cells[0]!.mix).toEqual([]);
  });
});

describe("words", () => {
  it("says how much a column rests on", () => {
    expect(sampleOf({ reviewed: 0 })).toBe("none");
    expect(sampleOf({ reviewed: 9 })).toBe("few");
    expect(sampleOf({ reviewed: 10 })).toBe("ok");
  });

  it("names a move as a player reads it, and links to the board at that ply", () => {
    expect(moveLabel({ ply: 27, side: "white", san: "Ng6+" })).toBe("14. Ng6+");
    expect(moveLabel({ ply: 26, side: "black", san: "Bxf3" })).toBe("13… Bxf3");
    expect(momentPath({ match_id: 29, ply: 27 })).toBe("/matches/29?ply=27");
  });

  it("says what a review costs", () => {
    expect(reviewCost(50)).toContain("roughly 6 minutes");
    expect(reviewCost(1)).toContain("1 game takes roughly 1 minute");
  });
});
