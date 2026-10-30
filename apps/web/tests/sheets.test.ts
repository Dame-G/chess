import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { leaders } from "../src/api/leaders";
import type { Profile, TrendPoint } from "../src/api/types";
import { bytes, plusMinus, progressText, reviewCost } from "../src/format";
import { addressQuery, compareGroups, leads, parseAddress, playerFromInput } from "../src/sheets/compare";
import { trendSeries } from "../src/sheets/trend";
import { profileKey, usePlayers, ALL_GAMES } from "../src/stores/players";

/** A profile with only what a test cares about; the rest is empty. */
function profile(patch: Partial<Profile> & { score?: number | null; acpl?: number | null } = {}): Profile {
  const wdl = { games: 10, wins: 5, draws: 0, losses: 5, score: patch.score ?? 50 };
  const rates = {
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
  return {
    player: { key: "you", name: "You", title: null, ratings: {}, lichess: null },
    filter: { speeds: [], rated: null, since: null, limit: 200 },
    games: 10,
    reviewed: 10,
    pending_review: 0,
    first_game: null,
    last_game: null,
    rating: 1800,
    results: { overall: wdl, white: wdl, black: wdl },
    accuracy: { overall: 80, opening: 80, middlegame: 80, endgame: 80, by_speed: [] },
    acpl: patch.acpl ?? 30,
    counts: rates,
    counts_per_game: rates,
    judgements: { inaccuracy: 0, mistake: 0, blunder: 0 },
    opponents: { average_rating: 1800, performance: 1800 },
    openings: [],
    time: {
      games: 10,
      average_ms: 4000,
      opening_ms: null,
      middlegame_ms: null,
      endgame_ms: null,
      time_trouble_blunders: 0,
      time_trouble_blunders_per_game: 0,
    },
    trend: [],
    longest_win_streak: 3,
    moments: { best: [], blunders: [], most_accurate_game: null, least_accurate_game: null },
    weak_spots: [],
    themes: [],
    ...patch,
  };
}

describe("the compare address", () => {
  it("reads players, speeds, rated and limit, and ignores the rest", () => {
    const a = parseAddress({
      players: "you,engine:Stockfish,lichess:CuteMouse83,nonsense,you",
      speeds: "blitz,bogus,rapid",
      rated: "true",
      limit: "50",
    });
    expect(a).toEqual({
      players: ["you", "engine:stockfish", "lichess:cutemouse83"],
      speeds: ["blitz", "rapid"],
      rated: true,
      limit: 50,
    });
  });

  it("starts with You and the analysis engine when nobody valid is named, and caps the count at five", () => {
    expect(parseAddress({}).players).toEqual(["you", "engine:stockfish"]);
    expect(parseAddress({ players: "?!" }, "lc0").players).toEqual(["you", "engine:lc0"]);
    expect(parseAddress({ players: "you,engine:a,lichess:aa,lichess:bb,lichess:cc,lichess:dd" }).players).toHaveLength(
      5,
    );
  });

  it("writes only what differs from the defaults", () => {
    expect(addressQuery({ players: ["you", "engine:stockfish"], speeds: [], rated: null, limit: 200 })).toEqual({
      players: "you,engine:stockfish",
    });
    expect(addressQuery({ players: ["you"], speeds: ["blitz"], rated: false, limit: 100 })).toEqual({
      players: "you",
      speeds: "blitz",
      rated: "false",
      limit: "100",
    });
  });

  it("takes a Lichess name, an @name or a pasted profile link", () => {
    expect(playerFromInput("CuteMouse83")).toBe("lichess:cutemouse83");
    expect(playerFromInput("@cutemouse83")).toBe("lichess:cutemouse83");
    expect(playerFromInput("https://lichess.org/@/CuteMouse83")).toBe("lichess:cutemouse83");
    expect(playerFromInput("lichess.org/@/some_one-1/")).toBe("lichess:some_one-1");
    expect(playerFromInput("lichess:x_y")).toBe("lichess:x_y");
    expect(playerFromInput("engine:Maia-1500")).toBe("engine:maia-1500");
    expect(playerFromInput("You")).toBe("you");
    expect(playerFromInput("")).toBeNull();
    expect(playerFromInput("a")).toBeNull();
    expect(playerFromInput("has space")).toBeNull();
    expect(playerFromInput("-leading")).toBeNull();
  });
});

describe("leaders", () => {
  it("picks the highest score and the lowest centipawn loss, ties together", () => {
    const l = leaders([
      profile({ score: 55, acpl: 30 }),
      profile({ score: 60, acpl: 20 }),
      profile({ score: 60, acpl: 25 }),
    ]);
    expect(l.find((x) => x.row === "score")?.players).toEqual([1, 2]);
    expect(l.find((x) => x.row === "acpl")?.players).toEqual([1]);
  });

  it("gives a row only one player has figures for to nobody", () => {
    const l = leaders([profile({ acpl: null }), profile({ acpl: 20 })]);
    expect(l.find((x) => x.row === "acpl")?.players).toEqual([]);
  });

  it("answers whether a profile leads a row, and a row without a leader leads nobody", () => {
    const cmp = { leaders: leaders([profile({ score: 40 }), profile({ score: 70 })]) };
    expect(leads(cmp, "score", 1)).toBe(true);
    expect(leads(cmp, "score", 0)).toBe(false);
    expect(leads(cmp, undefined, 1)).toBe(false);
  });
});

describe("the comparison's rows", () => {
  it("adds a row of accuracy for each speed some profile has played", () => {
    const withSpeeds = profile({
      accuracy: {
        overall: 80,
        opening: 80,
        middlegame: 80,
        endgame: 80,
        by_speed: [
          { speed: "rapid", games: 4, reviewed: 4, results: profile().results.overall, accuracy: 82, acpl: 20 },
        ],
      },
    });
    const groups = compareGroups([withSpeeds, profile()]);
    const ids = groups.flatMap((g) => g.rows.map((r) => r.id));
    expect(ids).toContain("acc-rapid");
    expect(ids).not.toContain("acc-bullet");
  });

  it("leaves a figure empty when nothing was reviewed", () => {
    const rows = compareGroups([profile({ reviewed: 0 })]).flatMap((g) => g.rows);
    const blunders = rows.find((r) => r.id === "pg-blunder")!;
    expect(blunders.get(profile({ reviewed: 0 }))).toBeNull();
    expect(blunders.get(profile())).toBe(0);
  });
});

describe("the trend", () => {
  const point = (i: number, accuracy: number | null): TrendPoint => ({
    game: i,
    played_at: new Date(Date.UTC(2026, 0, i + 1)).toISOString(),
    accuracy,
    rating: 1800 + i,
    points: 1,
  });

  it("averages the accuracy of the games that have one, over a window", () => {
    const t = trendSeries([point(0, 80), point(1, null), point(2, 90), point(3, 70)], 2);
    expect(t.average).toEqual([80, 80, 90, 80]);
    expect(t.rating).toEqual([1800, 1801, 1802, 1803]);
    expect(t.x[0]).toBe(Date.UTC(2026, 0, 1) / 1000);
  });

  it("has no average where nothing in the window was reviewed", () => {
    expect(trendSeries([point(0, null), point(1, null)]).average).toEqual([null, null]);
  });
});

describe("figures", () => {
  it("writes an estimate with half its interval", () => {
    expect(plusMinus({ elo: 3.14, low: 1, high: 5.2 })).toBe("+3.1 ± 2.1");
    expect(plusMinus({ elo: -9.6, low: -15, high: -4.2 })).toBe("−9.6 ± 5.4");
  });

  it("writes sizes and review costs", () => {
    expect(bytes(940_000_000)).toBe("940 MB");
    expect(bytes(2_340_000_000)).toBe("2.3 GB");
    expect(bytes(512)).toBe("512 B");
    expect(reviewCost(2)).toBe("about 40 s");
    expect(reviewCost(20)).toBe("about 7 min");
    expect(reviewCost(3708)).toBe("about 21 h");
    expect(reviewCost(9000)).toBe("about 2 days");
    expect(progressText({ done: 1200, total: 3708, unit: "games", eta_seconds: null, text: null })).toBe(
      "1,200 of 3,708 games",
    );
    expect(progressText({ done: 3, total: null, unit: "games", eta_seconds: null, text: "3 games read" })).toBe(
      "3 games read",
    );
    expect(progressText(null)).toBeNull();
  });
});

describe("the players store", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.unstubAllGlobals();
  });

  it("keys a profile by player and filter", () => {
    expect(profileKey("you")).toBe("you?limit=200");
    expect(profileKey("lichess:x", { ...ALL_GAMES, speeds: ["blitz"], rated: true })).toBe(
      "lichess%3Ax?speeds=blitz&rated=true&limit=200".replace("lichess%3Ax", "lichess:x"),
    );
  });

  it("loads a profile through the API and keeps it under its filter", async () => {
    const seen: string[] = [];
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: string) => {
        seen.push(input);
        return new Response(JSON.stringify(profile()));
      }),
    );
    const players = usePlayers();
    await players.loadProfile("lichess:x", { ...ALL_GAMES, speeds: ["blitz"] });
    expect(seen[0]).toContain("/api/players/lichess%3Ax/profile?speeds=blitz&limit=200");
    expect(players.profileOf("lichess:x", { ...ALL_GAMES, speeds: ["blitz"] })).toBeDefined();
    expect(players.profileOf("lichess:x")).toBeUndefined();
  });
});
