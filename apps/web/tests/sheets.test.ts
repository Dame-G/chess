import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { leaders } from "../src/api/leaders";
import type { BotSettings, Idea, Profile, TrendPoint, TrialSummary } from "../src/api/types";
import { bytes, plusMinus, progressText, reviewCost } from "../src/format";
import { rulesSummary, declineWords } from "../src/parts/bot";
import { blocks, inline } from "../src/parts/markdown";
import { addressQuery, compareGroups, leads, parseAddress, playerFromInput } from "../src/sheets/compare";
import { decidingTrial, gaugePosition, ideaElo, progressWord } from "../src/sheets/idea";
import { trendSeries } from "../src/sheets/trend";
import { tuneCost, tuneCostText } from "../src/sheets/tune";
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
    ...patch,
  };
}

describe("the compare address", () => {
  it("reads players, speeds, rated and limit, and ignores the rest", () => {
    const a = parseAddress({
      players: "you,Dameless,lichess:CuteMouse83,nonsense,you",
      speeds: "blitz,bogus,rapid",
      rated: "true",
      limit: "50",
    });
    expect(a).toEqual({
      players: ["you", "dameless", "lichess:cutemouse83"],
      speeds: ["blitz", "rapid"],
      rated: true,
      limit: 50,
    });
  });

  it("starts with You and Dameless when nobody valid is named, and caps the count at five", () => {
    expect(parseAddress({}).players).toEqual(["you", "dameless"]);
    expect(parseAddress({ players: "?!" }).players).toEqual(["you", "dameless"]);
    expect(parseAddress({ players: "you,dameless,lichess:aa,lichess:bb,lichess:cc,lichess:dd" }).players).toHaveLength(
      5,
    );
  });

  it("writes only what differs from the defaults", () => {
    expect(addressQuery({ players: ["you", "dameless"], speeds: [], rated: null, limit: 200 })).toEqual({
      players: "you,dameless",
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
    expect(playerFromInput("Dameless")).toBe("dameless");
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

describe("tunes", () => {
  it("counts two games a pair and spreads them over the cores", () => {
    const c = tuneCost(100, 8, 10);
    expect(c.games).toBe(1_600);
    expect(c.hours).toBeCloseTo(1.11, 1);
  });

  it("says what it costs in words, and that a short tune is not a finished one", () => {
    expect(tuneCostText(tuneCost(100, 8, 10))).toMatch(/1,600 games, about 1.1 hours/);
    expect(tuneCostText(tuneCost(1000, 8, 10))).toMatch(/16,000 games, about 11 hours/);
    expect(tuneCostText(tuneCost(1000, 8, 10))).toMatch(/95,000/);
    expect(tuneCostText(tuneCost(5000, 8, 8))).not.toMatch(/95,000/);
    expect(tuneCostText(tuneCost(10, 8, 15))).toMatch(/minutes/);
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

describe("notes in markdown", () => {
  it("reads inline bold, italics and code", () => {
    expect(inline("a **b** `c` *d*")).toEqual([
      { kind: "text", text: "a " },
      { kind: "bold", text: "b" },
      { kind: "text", text: " " },
      { kind: "code", text: "c" },
      { kind: "text", text: " " },
      { kind: "italic", text: "d" },
    ]);
  });

  it("splits paragraphs and lists, and never makes HTML", () => {
    const b = blocks("First line\nsecond line\n\n- one\n- two\n\n1. a\n2. b\n\n## Head\n<b>raw</b>");
    expect(b.map((x) => x.type)).toEqual(["p", "ul", "ol", "h", "p"]);
    expect(b[0]).toEqual({ type: "p", spans: [{ kind: "text", text: "First line second line" }] });
    expect(b[1]).toMatchObject({ type: "ul", items: [[{ text: "one" }], [{ text: "two" }]] });
    expect(b.at(-1)).toEqual({ type: "p", spans: [{ kind: "text", text: "<b>raw</b>" }] });
  });
});

describe("ideas", () => {
  const trial = (patch: Partial<TrialSummary> & Pick<TrialSummary, "id" | "stage" | "status">): TrialSummary => ({
    pairs: 0,
    llr: null,
    elo: null,
    detail: null,
    ...patch,
  });
  const idea = (trials: TrialSummary[], status: Idea["status"] = "kept"): Pick<Idea, "trials" | "status"> => ({
    trials,
    status,
  });

  it("takes the last trial with an estimate as the one that decided", () => {
    const i = idea([
      trial({ id: 1, stage: "screen", status: "passed", elo: { elo: 3, low: -2, high: 8 } }),
      trial({ id: 2, stage: "short", status: "passed", elo: { elo: 1.2, low: -0.8, high: 3.2 } }),
    ]);
    expect(decidingTrial(i)?.id).toBe(2);
    expect(ideaElo(i)).toBe("+1.2 ± 2.0");
  });

  it("has no Elo when no games decided it", () => {
    const i = idea([trial({ id: 1, stage: "fingerprint", status: "failed" })]);
    expect(decidingTrial(i)?.id).toBe(1);
    expect(ideaElo(i)).toBe("");
  });

  it("says where a running idea is", () => {
    expect(progressWord(idea([], "queued"))).toBe("Waiting its turn");
    expect(progressWord(idea([trial({ id: 1, stage: "short", status: "running" })], "running"))).toBe("Short games");
    expect(progressWord(idea([], "running"))).toBe("Starting");
  });

  it("puts the evidence between its lines", () => {
    expect(gaugePosition({ value: 0, lower: -2.94, upper: 2.94 })).toBeCloseTo(0.5);
    expect(gaugePosition({ value: 2.94, lower: -2.94, upper: 2.94 })).toBe(1);
    expect(gaugePosition({ value: -9, lower: -2.94, upper: 2.94 })).toBe(0);
  });
});

describe("the bot's rules in a sentence", () => {
  const settings: BotSettings = {
    version: "champion",
    hash: 64,
    threads: 1,
    move_overhead: 300,
    ponder: true,
    accept_rated: true,
    accept_casual: true,
    speeds: ["bullet", "blitz", "rapid"],
    min_initial: 60,
    max_initial: 1800,
    min_increment: 0,
    max_increment: 30,
    from_position: false,
    accept_people: true,
    accept_bots: true,
    max_games: 1,
    seek_clocks: ["3+0"],
    seek_rated: true,
    seek_range: 300,
  };

  it("names what it takes", () => {
    expect(rulesSummary(settings)).toBe(
      "Rated and casual Bullet, Blitz and Rapid games, from 1 minute to 30 minutes with up to 30 s added a move, from people and bots, one at a time. Plays the champion.",
    );
  });

  it("says no version is chosen rather than naming the champion", () => {
    const text = rulesSummary({ ...settings, version: null });
    expect(text).toMatch(/No version is chosen\.$/);
    expect(text).not.toMatch(/champion/);
  });

  it("follows the settings", () => {
    const text = rulesSummary({
      ...settings,
      accept_casual: false,
      accept_bots: false,
      speeds: ["rapid"],
      max_increment: 0,
      max_games: 2,
      version: "v13",
    });
    expect(text).toBe(
      "Rated Rapid games, from 1 minute to 30 minutes with no added time, from people only, 2 at a time. Plays v13.",
    );
  });

  it("writes Lichess's decline keys in words", () => {
    expect(declineWords("tooFast")).toBe("too fast");
    expect(declineWords("later")).toBe("busy right now");
    expect(declineWords("odd")).toBe("odd");
    expect(declineWords(null)).toBe("");
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

describe("an idea's evidence over time", () => {
  it("collects the evidence at each pair count from trial progress", async () => {
    setActivePinia(createPinia());
    const { dispatch } = await import("../src/api/stream");
    const { useIdeas } = await import("../src/stores/ideas");
    const ideas = useIdeas();
    const llr = (value: number) => ({ value, lower: -2.94, upper: 2.94 });
    let id = 1;
    const progress = (penta: [number, number, number, number, number], value: number) =>
      dispatch({
        id: id++,
        topic: "ideas",
        kind: "trial.progress",
        data: { id: 7, idea: 1, penta, wdl: [0, 0, 0], llr: llr(value), elo: null },
      });
    progress([1, 2, 3, 2, 1], 0.1);
    progress([1, 2, 3, 2, 1], 0.1);
    progress([2, 2, 3, 2, 1], 0.3);
    expect(ideas.llrHistory[7]).toEqual([
      [9, 0.1],
      [10, 0.3],
    ]);
  });
});
