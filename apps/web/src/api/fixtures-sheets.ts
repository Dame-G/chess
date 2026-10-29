/**
 * The fixtures' answers for the sheets: profiles and comparisons, the leaderboard, ideas and tunes, Dameless's
 * strength, the connections and the machine. `fixtures.ts` hands this module the data it shares (games, ideas, trials,
 * versions, work, connections) and the way to speak on the event stream; everything here is invented too.
 */
import { error, reply } from "./fixtures-http";
import { settingsRoute } from "./fixtures-settings";
import { applyVariants, tuneVariant, variant } from "./fixtures-variants";
import { leaders } from "./leaders";
import type {
  BotSettings,
  Classification,
  ClassCounts,
  Connections,
  ConnectionStatus,
  DamelessView,
  EventKind,
  EventPayloads,
  Game,
  GameId,
  Idea,
  IdeaId,
  Moment,
  NewIdeaRequest,
  Phase,
  Player,
  PlayerKey,
  Priority,
  Profile,
  ProfileFilter,
  Speed,
  Status,
  ProgressPoint,
  LeaderboardEntry,
  TimeControl,
  Trial,
  TrialSummary,
  Tunable,
  TuneParamInfo,
  TuneDetail,
  TuneStep,
  Version,
  WeakSpot,
  Wdl,
  Work,
} from "./types";

export interface SheetContext {
  minutesAgo: (m: number) => string;
  rng: (seed: number) => () => number;
  emit: <K extends EventKind>(kind: K, topic: string, data: EventPayloads[K]) => void;
  ideas: Idea[];
  trials: Trial[];
  versions: Version[];
  work: Work[];
  connections: Connections;
  status: Status;
  games: Map<GameId, { game: Game }>;
  summaryOf: (t: Trial) => TrialSummary;
  /** The position after the first `plies` moves of a game written in SAN, as a FEN. */
  fenAfter: (san: string, plies: number) => string;
  lines: { italian: string; sicilian: string; qgd: string; london: string; caro: string };
  /** `botnoversion`, `botversion`: the bot is offline, with no version chosen, or with the champion chosen. */
  /** `unlinked`: You has not logged in to Lichess yet. */
  mode: string | null;
}

type Handler = (
  method: string,
  path: string,
  url: URL,
  body: Record<string, unknown>,
) => Response | undefined | Promise<Response>;

// ---- Players

interface Roster {
  name: string;
  title: string | null;
  rating: Partial<Record<Speed, number>>;
  games: number;
  reviewed: number;
  accuracy: number;
  win: number;
  draw: number;
  seed: number;
}

const SHARE: Partial<Record<Speed, number>> = { bullet: 0.18, blitz: 0.55, rapid: 0.24, classical: 0.03 };
const SPEED_ACCURACY: Partial<Record<Speed, number>> = { bullet: -4.2, blitz: 0, rapid: 2.8, classical: 3.5 };

const LEADERBOARD: Record<string, [string, string | null, number][]> = {
  bullet: [
    ["Zhigalko_Sergei", "GM", 3218],
    ["nihalsarin2004", "GM", 3187],
    ["DrNykterstein", "GM", 3172],
    ["penguingim1", "GM", 3139],
    ["Msb2", "GM", 3101],
    ["Ghost_of_Morphy", null, 3077],
    ["opperwezen", "IM", 3044],
    ["lance5500", "LM", 3012],
    ["Fins", "GM", 2987],
    ["Jurasek", null, 2961],
  ],
  blitz: [
    ["DrNykterstein", "GM", 3098],
    ["nihalsarin2004", "GM", 3071],
    ["Zhigalko_Sergei", "GM", 3040],
    ["Msb2", "GM", 3012],
    ["lovlas", "IM", 2985],
    ["penguingim1", "GM", 2963],
    ["Fins", "GM", 2940],
    ["opperwezen", "IM", 2912],
    ["may6enexttime", "GM", 2893],
    ["BabaYagaa", null, 2871],
  ],
  rapid: [
    ["DrNykterstein", "GM", 2980],
    ["alireza2003", "GM", 2951],
    ["nihalsarin2004", "GM", 2930],
    ["Msb2", "GM", 2904],
    ["lovlas", "IM", 2880],
    ["Fins", "GM", 2861],
    ["penguingim1", "GM", 2837],
    ["zhigalko_sergei", "GM", 2815],
    ["Oleksandr_Bortnyk", "GM", 2794],
    ["Jurasek", null, 2768],
  ],
};

const OPENING_FAMILIES = [
  "Italian Game",
  "Sicilian Defense",
  "French Defense",
  "Caro-Kann Defense",
  "Queen's Gambit Declined",
  "London System",
  "Scandinavian Defense",
  "Philidor Defense",
];

function wdl(games: number, win: number, draw: number): Wdl {
  const wins = Math.round(games * win);
  const draws = Math.round(games * draw);
  const losses = Math.max(0, games - wins - draws);
  const played = wins + draws + losses;
  return {
    games,
    wins,
    draws,
    losses,
    score: played ? Math.round((1000 * (wins + draws / 2)) / played) / 10 : null,
  };
}

const classes: Classification[] = [
  "brilliant",
  "great",
  "best",
  "excellent",
  "good",
  "book",
  "inaccuracy",
  "mistake",
  "blunder",
  "miss",
];

function emptyCounts(): ClassCounts {
  return Object.fromEntries(classes.map((c) => [c, 0])) as ClassCounts;
}

const round = (n: number, d = 1) => Math.round(n * 10 ** d) / 10 ** d;

export function createSheetRoutes(ctx: SheetContext): Handler {
  const { emit, minutesAgo } = ctx;

  // ---- The people

  const roster = new Map<string, Roster>();
  roster.set("you", {
    name: "dame_g",
    title: null,
    rating: { bullet: 1722, blitz: 1840, rapid: 1846, classical: 1790 },
    games: 64,
    reviewed: 41,
    accuracy: 79.4,
    win: 0.53,
    draw: 0.08,
    seed: 101,
  });
  roster.set("dameless", {
    name: "Dameless",
    title: "BOT",
    rating: { bullet: 2281, blitz: 2304, rapid: 2310 },
    games: 180,
    reviewed: 180,
    accuracy: 93.6,
    win: 0.71,
    draw: 0.15,
    seed: 202,
  });
  roster.set("lichess:cutemouse83", {
    name: "cutemouse83",
    title: null,
    rating: { blitz: 1871, rapid: 1862 },
    games: 0,
    reviewed: 0,
    accuracy: 82.1,
    win: 0.5,
    draw: 0.06,
    seed: 303,
  });
  roster.set("lichess:knightowl", {
    name: "knightowl",
    title: "FM",
    rating: { blitz: 1802, rapid: 1790 },
    games: 22,
    reviewed: 22,
    accuracy: 84.9,
    win: 0.55,
    draw: 0.1,
    seed: 404,
  });

  // A long name and a thousand games, to see how the player sheets and Compare hold up.
  roster.set("lichess:the_quick_brown_fox_jumps_over", {
    name: "The_Quick_Brown_Fox_Jumps_Over",
    title: "GM",
    rating: { bullet: 2701, blitz: 2744, rapid: 2688, classical: 2610 },
    games: 1243,
    reviewed: 1243,
    accuracy: 91.2,
    win: 0.63,
    draw: 0.2,
    seed: 505,
  });

  // States to look at: `lichess:broken…` fails to load, `lichess:slow…` loads slowly, `lichess:nogames` has no games
  // here; You is the same with `?fixtures=youbroken`, `?fixtures=slow` and `?fixtures=newcomer` (no games at all).
  roster.set("lichess:nogames", {
    name: "nogames",
    title: null,
    rating: { rapid: 1500 },
    games: 0,
    reviewed: 0,
    accuracy: 80,
    win: 0.5,
    draw: 0.05,
    seed: 606,
  });
  if (ctx.mode === "newcomer") Object.assign(roster.get("you")!, { games: 0, reviewed: 0 });
  const profileQuirk = (key: string): "broken" | "slow" | null =>
    /^lichess:broken/.test(key) || (key === "you" && ctx.mode === "youbroken")
      ? "broken"
      : /^lichess:slow/.test(key) || (key === "you" && ctx.mode === "slow")
        ? "slow"
        : null;

  /** A name Lichess knows: anything shaped like a name, except the ones the fixtures say are not there. */
  const exists = (key: string) => !/^lichess:(nosuch|nobody|zzz)/.test(key);

  function entry(key: string): Roster | undefined {
    const known = roster.get(key);
    if (known) return known;
    const m = key.match(/^lichess:(.+)$/);
    if (!m || !exists(key)) return undefined;
    const board = Object.values(LEADERBOARD)
      .flat()
      .find(([n]) => n.toLowerCase() === m[1]);
    const seed = [...m[1]!].reduce((a, c) => a * 31 + c.charCodeAt(0), 7) % 9973;
    const r = ctx.rng(seed);
    const entry: Roster = {
      name: board?.[0] ?? m[1]!,
      title: board?.[1] ?? null,
      rating: { blitz: board?.[2] ?? 1500 + Math.round(r() * 700), rapid: 1500 + Math.round(r() * 700) },
      games: 0,
      reviewed: 0,
      accuracy: 78 + r() * 12,
      win: 0.5 + (r() - 0.5) * 0.2,
      draw: 0.08,
      seed,
    };
    roster.set(key, entry);
    return entry;
  }

  function player(key: string): Player | undefined {
    const r = entry(key);
    if (!r) return undefined;
    const ratings: Player["ratings"] = {};
    for (const [speed, rating] of Object.entries(r.rating) as [keyof Player["ratings"], number][])
      ratings[speed] = { rating, games: 120 + Math.round(rating / 10), provisional: false };
    return {
      key: key as PlayerKey,
      name: r.name,
      title: r.title,
      ratings,
      lichess: key.startsWith("lichess:") ? r.name : key === "you" ? r.name : key === "dameless" ? "DamelessBot" : null,
    };
  }

  /** The games, from the roster, that a filter lets through. */
  function build(key: string, filter: ProfileFilter): Profile | undefined {
    const r = entry(key);
    const p = player(key);
    if (!r || !p) return undefined;
    const shares = (Object.keys(SHARE) as Speed[]).filter(
      (s) => r.rating[s] !== undefined && (!filter.speeds?.length || filter.speeds.includes(s)),
    );
    const shareSum = shares.reduce((a, s) => a + (SHARE[s] ?? 0), 0);
    const ratedShare = filter.rated == null ? 1 : filter.rated ? 0.82 : 0.18;
    const games = Math.min(filter.limit, Math.round(r.games * shareSum * ratedShare));
    const reviewed = r.games ? Math.min(games, Math.round((r.reviewed / r.games) * games)) : 0;
    const rnd = ctx.rng(r.seed);
    const rating = r.rating.blitz ?? r.rating.rapid ?? 1500;

    const results = {
      overall: wdl(games, r.win, r.draw),
      white: wdl(Math.ceil(games / 2), r.win + 0.04, r.draw),
      black: wdl(Math.floor(games / 2), r.win - 0.04, r.draw),
    };

    const acc = r.accuracy;
    const accOf = (offset: number) => (reviewed ? round(Math.min(99.4, acc + offset)) : null);
    const by_speed = shares.map((s) => {
      const g = Math.round((games * (SHARE[s] ?? 0)) / (shareSum || 1));
      const rv = Math.min(g, Math.round((reviewed * g) / (games || 1)));
      return {
        speed: s,
        games: g,
        reviewed: rv,
        results: wdl(g, r.win, r.draw),
        accuracy: rv ? round(Math.min(99.4, acc + (SPEED_ACCURACY[s] ?? 0))) : null,
        acpl: rv ? round((100 - (acc + (SPEED_ACCURACY[s] ?? 0))) * 1.7) : null,
      };
    });

    const per = (x: number) => (reviewed ? round(x, 2) : 0);
    const lossy = Math.max(0.1, (96 - acc) / 5);
    const rates = {
      brilliant: per(0.05 + (acc - 70) / 400),
      great: per(0.3 + (acc - 70) / 120),
      best: per(11 + (acc - 70) / 3),
      excellent: per(7),
      good: per(8 - (acc - 70) / 8),
      book: per(4.4),
      inaccuracy: per(lossy * 1.3),
      mistake: per(lossy * 0.75),
      blunder: per(lossy * 0.42),
      miss: per(lossy * 0.2),
    };
    const counts = emptyCounts();
    for (const c of classes) counts[c] = Math.round(rates[c] * reviewed);

    const avgOpp = games ? Math.round(rating - 18 + (rnd() - 0.5) * 40) : null;
    const decided = results.overall.wins + results.overall.draws + results.overall.losses;
    const performance =
      avgOpp !== null && decided
        ? Math.round(avgOpp + (400 * (results.overall.wins - results.overall.losses)) / decided)
        : null;

    const families = OPENING_FAMILIES.slice(0, 7).map((name, i) => {
      const share = [0.26, 0.2, 0.15, 0.13, 0.1, 0.09, 0.07][i]!;
      const g = Math.round(games * share);
      return { name, results: wdl(g, r.win + (rnd() - 0.5) * 0.3, r.draw) };
    });

    const span = Math.min(games, 60);
    const trend: Profile["trend"] = [];
    let walk = rating - 26;
    for (let i = 0; i < span; i++) {
      walk += (rnd() - 0.46) * 9;
      const scored = rnd() < r.win + r.draw / 2 ? (rnd() < r.draw / (r.win + r.draw) ? 0.5 : 1) : 0;
      trend.push({
        game: 1000 + i,
        played_at: minutesAgo((span - i) * 60 * 30 + Math.round(rnd() * 600)),
        accuracy:
          i >= span - Math.round((reviewed / (games || 1)) * span)
            ? round(acc + (rnd() - 0.5) * 16 + (i - span / 2) * 0.05)
            : null,
        rating: Math.round(walk),
        points: scored,
      });
    }

    const moment = (
      game: GameId,
      ply: number,
      san: string,
      classification: Classification,
      before: number,
      after: number,
      opponent: string,
      sacrifice = 0,
    ): Moment => ({
      game,
      ply,
      san,
      side: "white",
      classification,
      win_before: before,
      win_after: after,
      sacrifice,
      opponent,
      played_at: minutesAgo(60 * (game + 3)),
      url: `https://lichess.org/g${game}#${ply}`,
    });
    const has = reviewed > 0;
    const moments: Profile["moments"] = {
      best: has
        ? [
            moment(3, 24, "Rxf7", "brilliant", 52.4, 71.8, "Dameless v13", 2),
            moment(4, 31, "Qd6+", "great", 41.2, 59.3, "cutemouse83"),
          ]
        : [],
      blunders: has
        ? [
            moment(5, 39, "Qh5", "blunder", 64.0, 18.5, "Stockfish level 5"),
            moment(4, 22, "Bxh7", "blunder", 55.1, 21.0, "cutemouse83"),
            moment(7, 47, "Rd1", "blunder", 47.8, 19.4, "knightowl"),
          ]
        : [],
      most_accurate_game: has
        ? { game: 3, accuracy: 94.1, opponent: "Dameless v13", outcome: "white_wins", played_at: minutesAgo(60 * 20) }
        : null,
      least_accurate_game: has
        ? { game: 5, accuracy: 58.3, opponent: "Stockfish level 5", outcome: "draw", played_at: minutesAgo(60 * 48) }
        : null,
    };

    const phase = (
      phase: Phase,
      fen: string,
      played: string,
      best: string,
      swing: number,
      ply: number,
      recurring: number,
      opening: string,
      game: GameId,
    ): WeakSpot => ({
      game,
      ply,
      fen,
      played,
      best,
      swing,
      opening,
      phase,
      recurring,
    });
    const L = ctx.lines;
    const weak_spots: WeakSpot[] =
      key === "you" && has
        ? [
            phase("opening", ctx.fenAfter(L.italian, 9), "h6", "Bb3", 14.2, 10, 4, "Italian Game", 3),
            phase("opening", ctx.fenAfter(L.italian, 7), "d6", "O-O", 11.6, 8, 4, "Italian Game", 4),
            phase("opening", ctx.fenAfter(L.sicilian, 7), "a6", "Be2", 12.9, 8, 3, "Sicilian Defense", 7),
            phase("middlegame", ctx.fenAfter(L.caro, 17), "Qh5", "Nbd2", 45.5, 18, 2, "Caro-Kann Defense", 5),
            phase("middlegame", ctx.fenAfter(L.qgd, 21), "Rd1", "Bxf6", 28.4, 22, 2, "Queen's Gambit Declined", 5),
            phase("endgame", ctx.fenAfter(L.london, 31), "Kf2", "Kd2", 17.3, 32, 1, "London System", 7),
          ]
        : [];

    return {
      player: p,
      filter,
      games,
      reviewed,
      pending_review: games - reviewed,
      unreviewed_total: r.games - r.reviewed,
      first_game: games ? minutesAgo(span * 60 * 30 + 600) : null,
      last_game: games ? minutesAgo(30) : null,
      rating: games ? rating : null,
      results,
      accuracy: {
        overall: accOf(0),
        opening: accOf(4.6),
        middlegame: accOf(-3.1),
        endgame: accOf(-1.8),
        by_speed,
      },
      acpl: reviewed ? round((100 - acc) * 1.7) : null,
      counts,
      counts_per_game: rates,
      judgements: {
        inaccuracy: counts.inaccuracy,
        mistake: counts.mistake + Math.round(counts.miss / 2),
        blunder: counts.blunder + Math.round(counts.miss / 2),
      },
      opponents: { average_rating: avgOpp, performance },
      openings: families.filter((f) => f.results.games > 0),
      time: {
        games: reviewed,
        average_ms: reviewed ? Math.round(4200 + (rnd() - 0.5) * 1500) : null,
        opening_ms: reviewed ? 1900 : null,
        middlegame_ms: reviewed ? 5600 : null,
        endgame_ms: reviewed ? 3400 : null,
        time_trouble_blunders: reviewed ? Math.round(reviewed * 0.18) : 0,
        time_trouble_blunders_per_game: reviewed ? 0.18 : 0,
      },
      trend,
      longest_win_streak: games ? 3 + Math.round(rnd() * 5) : 0,
      moments,
      weak_spots,
    };
  }

  // ---- Background work for players

  function startWork(
    title: string,
    kind: "import" | "review",
    key: string,
    total: number,
    then: () => void,
    priority: Priority = kind === "review" ? "review" : "interactive",
  ) {
    const id = Math.max(...ctx.work.map((w) => w.id)) + 1;
    const w: Work = {
      id,
      kind,
      status: "running",
      priority,
      title,
      subject: { type: "player", key: key as PlayerKey },
      cores: 0,
      progress: { done: 0, total, unit: "games", eta_seconds: 6, text: null },
      paused_for: null,
      error: null,
      created_at: new Date().toISOString(),
      started_at: new Date().toISOString(),
      ended_at: null,
    };
    ctx.work.push(w);
    emit("work.upserted", "work", { ...w });
    let step = 0;
    const timer = setInterval(() => {
      if (w.status === "stopped") {
        clearInterval(timer);
        return;
      }
      step++;
      w.progress = {
        done: Math.round((total * step) / 4),
        total,
        unit: "games",
        eta_seconds: (4 - step) * 1.5,
        text: null,
      };
      if (step >= 4) {
        clearInterval(timer);
        then();
        w.status = "done";
        w.ended_at = new Date().toISOString();
      }
      emit("work.upserted", "work", { ...w });
    }, 1500);
    return w;
  }

  // ---- Dameless

  function strength(): ProgressPoint[] {
    let elo = 0;
    let variance = 0;
    return ctx.versions
      .filter((v) => v.line.type === "champion")
      .sort(
        (a, b) => (a.line.type === "champion" ? a.line.number : 0) - (b.line.type === "champion" ? b.line.number : 0),
      )
      .map((v) => {
        if (v.elo) {
          elo += v.elo.elo;
          variance += ((v.elo.high - v.elo.low) / 2) ** 2;
        }
        const half = Math.sqrt(variance);
        return {
          at: v.created_at,
          value: round(elo),
          low: round(elo - half),
          high: round(elo + half),
          game: null,
          version: v.id,
          label: v.name,
        };
      });
  }

  function overview(): DamelessView {
    const line = ctx.versions.filter((v) => v.line.type === "champion");
    const sorted = [...ctx.ideas].sort((a, b) => b.created_at.localeCompare(a.created_at));
    return {
      champion: line.reduce((a, b) =>
        (b.line.type === "champion" ? b.line.number : 0) > (a.line.type === "champion" ? a.line.number : 0) ? b : a,
      ),
      line: variant("empty") ? line.slice(0, 1) : line,
      upstream: null,
      ideas: {
        running: sorted.filter((i) => i.status === "running"),
        queued: sorted.filter((i) => i.status === "queued"),
        recent: sorted.filter((i) => i.status !== "running" && i.status !== "queued").slice(0, 6),
      },
      connection: ctx.connections.bot.bot ?? null,
      strength: variant("empty") ? [] : strength(),
    };
  }

  // ---- Ideas added to the shared fixtures

  const stc: TimeControl = { type: "clock", initial_ms: 8000, increment_ms: 80 };
  const trial = (t: Partial<Trial> & Pick<Trial, "id" | "idea" | "stage" | "status">): Trial => ({
    preset: null,
    time: null,
    played: null,
    scaling: null,
    bounds: null,
    penta: [0, 0, 0, 0, 0],
    wdl: [0, 0, 0],
    llr: null,
    elo: null,
    nelo: null,
    detail: null,
    started_at: null,
    ended_at: null,
    ...t,
  });
  const idea = (i: Partial<Idea> & Pick<Idea, "id" | "title" | "kind" | "branch" | "status">): Idea => ({
    hypothesis: "",
    author: "claude",
    base: 14,
    version: null,
    verdict: null,
    trials: [],
    tags: [],
    notes: null,
    created_at: minutesAgo(60),
    ended_at: null,
    ...i,
  });

  ctx.trials.push(
    trial({
      id: 20,
      idea: 4,
      stage: "tune",
      status: "running",
      time: stc,
      detail: "Iteration 140 of 1,000.",
      started_at: minutesAgo(60 * 3),
    }),
    trial({ id: 21, idea: 5, stage: "fingerprint", status: "passed", detail: "Bench identical: 1,672,794 nodes." }),
    trial({
      id: 22,
      idea: 5,
      stage: "speed",
      status: "passed",
      detail: "1.9 % faster: 538,200 against 528,100 nodes a second, over 20 paired runs.",
      started_at: minutesAgo(60 * 6),
      ended_at: minutesAgo(60 * 6 - 4),
    }),
    trial({
      id: 23,
      idea: 7,
      stage: "fingerprint",
      status: "passed",
      detail: "Bench changed.",
      ended_at: minutesAgo(60 * 70),
    }),
    trial({
      id: 24,
      idea: 7,
      stage: "screen",
      status: "stopped",
      preset: "vstc",
      time: { type: "clock", initial_ms: 4000, increment_ms: 40 },
      bounds: [-5, 0],
      penta: [6, 31, 70, 28, 5],
      wdl: [212, 260, 214],
      llr: { value: 0.42, lower: -2.94, upper: 2.94 },
      elo: { elo: 0.4, low: -10.5, high: 11.3 },
      detail: "Stopped by you after 140 pairs.",
      started_at: minutesAgo(60 * 70),
      ended_at: minutesAgo(60 * 69),
    }),
    trial({
      id: 25,
      idea: 8,
      stage: "fingerprint",
      status: "failed",
      detail: "The branch did not build: cannot find value `margin` in this scope.",
      ended_at: minutesAgo(60 * 90),
    }),
  );

  ctx.versions.push({
    id: 22,
    name: "cheaper-history",
    commit: "c4eap3r".padEnd(40, "0"),
    branch: "cheaper-history",
    line: { type: "idea", idea: 5 },
    parent: 14,
    bench: 1_672_794,
    elo: null,
    created_at: minutesAgo(60 * 6),
  });

  ctx.ideas.push(
    idea({
      id: 4,
      title: "Retune search constants",
      hypothesis:
        "Eight pruning and reduction constants have not moved since v6; tuning them together should find a few Elo.",
      kind: "tune",
      author: "you",
      branch: "spsa/tunables",
      status: "running",
      tags: ["tune-oct"],
      notes: "Starting from the declared defaults. Iterations: 1,000 of 8 pairs.",
      created_at: minutesAgo(60 * 3),
    }),
    idea({
      id: 5,
      title: "Cheaper history update",
      hypothesis: "Updating the continuation history only for moves that raise alpha saves time without changing play.",
      kind: "speed",
      branch: "cheaper-history",
      version: 22,
      status: "kept",
      verdict: "Kept: 1.9 % faster, identical search.",
      notes:
        "The bench is identical, so the search is unchanged.\n\n- Skips the write for moves that fail low.\n- No change to the table's size.",
      created_at: minutesAgo(60 * 6),
      ended_at: minutesAgo(60 * 6 - 4),
    }),
    idea({
      id: 6,
      title: "Futility margin when improving",
      hypothesis: "A smaller margin when the static eval is improving prunes more of the quiet tail.",
      kind: "gain",
      branch: "futility-improving",
      status: "queued",
      tags: ["search-oct"],
      created_at: minutesAgo(25),
    }),
    idea({
      id: 7,
      title: "Singular extension at PV nodes",
      hypothesis: "Extending singular moves at PV nodes by two plies finds more tactics.",
      kind: "gain",
      author: "you",
      branch: "se-pv",
      status: "stopped",
      verdict: "Stopped by you at the screen.",
      created_at: minutesAgo(60 * 70),
      ended_at: minutesAgo(60 * 69),
    }),
    idea({
      id: 8,
      title: "Margin by depth in razoring",
      hypothesis: "Scaling the razoring margin with depth removes a special case.",
      kind: "neutral",
      branch: "razor-depth",
      status: "failed",
      verdict: "Failed: the branch did not build.",
      created_at: minutesAgo(60 * 90),
      ended_at: minutesAgo(60 * 90 - 1),
    }),
  );
  for (const i of ctx.ideas) i.trials = ctx.trials.filter((t) => t.idea === i.id).map(ctx.summaryOf);
  applyVariants(ctx);

  // ---- Tunes

  const TUNABLES: Tunable[] = [
    ["rfp_margin", 76, 20, 200],
    ["rfp_depth", 8, 3, 14],
    ["nmp_base", 4, 2, 8],
    ["lmr_base", 98, 40, 160],
    ["lmr_div", 215, 120, 320],
    ["lmp_quad", 12, 4, 30],
    ["futility_base", 118, 40, 240],
    ["history_bonus", 168, 60, 300],
  ].map(([name, start, min, max]) => ({
    name: name as string,
    start: start as number,
    min: min as number,
    max: max as number,
    c_end: Math.max(1, Math.round(((max as number) - (min as number)) / 20)),
    r_end: 0.002,
  }));
  /** What `GET /api/ideas/{id}/tune` says of each parameter. */
  const paramInfo = (t: Tunable): TuneParamInfo => ({ name: t.name, start: t.start, min: t.min, max: t.max });

  function tuneState(id: IdeaId): TuneDetail {
    const rnd = ctx.rng(900 + id);
    const length = tuneVariant();
    const params = TUNABLES;
    const steps: TuneStep[] = [];
    const target = params.map((p) => p.start + (rnd() - 0.5) * (p.max - p.min) * 0.35);
    const values = params.map((p) => p.start);
    for (let k = 1; k <= (length?.steps ?? 140); k++) {
      params.forEach((p, i) => {
        const pull = (target[i]! - values[i]!) * 0.035;
        values[i] = Math.max(p.min, Math.min(p.max, values[i]! + pull + (rnd() - 0.5) * (p.max - p.min) * 0.012));
      });
      steps.push({
        iteration: k,
        values: values.map((v) => round(v, 2)),
        result: Math.round((rnd() - 0.45) * 6),
        penta: [1, 2, 3, 2, 0],
      });
    }
    return {
      spec: {
        params: [],
        iterations: length?.iterations ?? 1000,
        pairs: 8,
        time: stc,
        hash: 16,
        concurrency: null,
        alpha: 0.602,
        gamma: 0.101,
        a_ratio: 0.1,
      },
      params: params.map(paramInfo),
      steps,
    };
  }
  const tunes = new Map<IdeaId, TuneDetail>();
  const tuneTimers = new Map<IdeaId, ReturnType<typeof setInterval>>();

  function followTune(id: IdeaId, state: TuneDetail) {
    if (tuneTimers.has(id) || ctx.ideas.find((i) => i.id === id)?.status !== "running") return;
    const rnd = ctx.rng(state.steps.length + 5);
    tuneTimers.set(
      id,
      setInterval(() => {
        const last = state.steps.at(-1)!;
        const values = last.values.map((v, i) => {
          const p = TUNABLES[i]!;
          return round(Math.max(p.min, Math.min(p.max, v + (rnd() - 0.5) * (p.max - p.min) * 0.01)), 2);
        });
        const step: TuneStep = {
          iteration: last.iteration + 1,
          values,
          result: Math.round((rnd() - 0.45) * 6),
          penta: [1, 2, 3, 2, 0],
        };
        emit("tune.progress", `idea:${id}`, { idea: id, iteration: step.iteration, values, result: step.result });
      }, 2500),
    );
  }

  /** Makes the next idea's number and queues it, then lets it start a moment later. */
  function createIdea(body: NewIdeaRequest): Idea {
    const id = Math.max(...ctx.ideas.map((i) => i.id)) + 1;
    const made = idea({
      id,
      title: body.title,
      hypothesis: body.hypothesis ?? "",
      kind: body.kind ?? "gain",
      author: body.author ?? "you",
      branch: body.branch,
      status: "queued",
      tags: body.tags ?? [],
      created_at: new Date().toISOString(),
    });
    ctx.ideas.push(made);
    emit("idea.upserted", "ideas", structuredClone(made));
    setTimeout(() => {
      made.status = "running";
      const t = trial({
        id: Math.max(...ctx.trials.map((x) => x.id)) + 1,
        idea: id,
        stage: body.kind === "tune" ? "tune" : body.kind === "net" ? "train" : "fingerprint",
        status: "running",
        started_at: new Date().toISOString(),
      });
      ctx.trials.push(t);
      made.trials = [ctx.summaryOf(t)];
      emit("trial.upserted", "ideas", structuredClone(t));
      emit("idea.upserted", "ideas", structuredClone(made));
    }, 2500);
    return made;
  }

  function promote(found: Idea): Response {
    if (found.status !== "kept") return error(409, "conflict", "Only a kept idea can be promoted.");
    const current = ctx.versions.filter((v) => v.line.type === "champion").length;
    const decided = [...found.trials].reverse().find((t) => t.elo);
    const next: Version = {
      id: current + 1,
      name: `v${current + 1}`,
      commit: (found.version != null ? (ctx.versions.find((v) => v.id === found.version)?.commit ?? "") : "").padEnd(
        40,
        "0",
      ),
      branch: "main",
      line: { type: "champion", number: current + 1 },
      parent: current,
      bench: 1_672_794,
      elo: decided?.elo ?? { elo: 1.9, low: 0.2, high: 3.6 },
      created_at: new Date().toISOString(),
    };
    ctx.versions.push(next);
    found.version = next.id;
    emit("version.upserted", "versions", structuredClone(next));
    emit("idea.upserted", "ideas", structuredClone(found));
    return reply("/api/ideas/{id}/promote", "post", structuredClone(next));
  }

  // ---- Connections

  const youAccount: ConnectionStatus["account"] = ctx.connections.you.account;
  if (ctx.mode === "unlinked") {
    ctx.connections.you = {
      key: "you",
      configured: false,
      account: null,
      state: "off",
      error: null,
      playing: [],
      challenges: [],
      you: null,
      bot: null,
    };
  }
  // `botnoversion` and `botversion`: the bot is linked and offline, with no version chosen / with the champion chosen.
  if ((ctx.mode === "botnoversion" || ctx.mode === "botversion") && ctx.connections.bot.bot) {
    const b = ctx.connections.bot;
    b.state = "off";
    b.playing = [];
    b.bot!.online = false;
    b.bot!.seek = { mode: "off", pending: null, note: null };
    b.bot!.settings.version = ctx.mode === "botversion" ? "champion" : null;
  }
  const connectionsEmit = (c: ConnectionStatus) => {
    emit("connection.upserted", `connection:${c.key}`, structuredClone(c));
    return reply("/api/connections/bot/online", "post", structuredClone(c));
  };
  const bot = () => ctx.connections.bot;

  // A few answered challenges for the bot's card, once the stream is open.
  setTimeout(() => {
    const answered = (
      id: string,
      name: string,
      rating: number,
      event: "accepted" | "declined",
      reason: string | null,
    ) =>
      emit("challenge", "connection:bot", {
        challenge: {
          id,
          connection: "bot",
          direction: "incoming",
          opponent: { name, title: "BOT", rating },
          rated: true,
          speed: "blitz",
          variant: "standard",
          time: { type: "clock", initial_ms: 180_000, increment_ms: 2000 },
          color: "random",
          playable: true,
          url: `https://lichess.org/${id}`,
        },
        event,
        reason,
        detail: null,
      });
    answered("ch1", "Bot-Sam", 2190, "accepted", null);
    answered("ch2", "FastBot", 2012, "declined", "tooFast");
    answered("ch3", "Vera-Bot", 2377, "declined", "later");
  }, 3500);

  const status = ctx.status;
  if (variant("nogpu")) status.ledger.gpus = 0;
  status.machine = {
    ...status.machine,
    engine: { dir: "C:\\dev\\reckless", found: true, branch: "main", commit: "9e3a5e41b0c2f87d", dirty: false },
    data_dir: "C:\\Users\\dame\\AppData\\Local\\dameless",
    builds: { count: 23, bytes: 2_340_000_000 },
  };

  return (method, path, url, body) => {
    let m: RegExpMatchArray | null;
    if (method === "GET") {
      const edge = settingsRoute(ctx.mode, path);
      if (edge) return edge;
      if (path === "/api/dameless") return reply("/api/dameless", "get", structuredClone(overview()));
      if (path === "/api/token") return reply("/api/token", "get", { token: "dmls_4f8c0a2b9d7e41c6a3b5e8f20d19c7a4" });
      if (path === "/api/tunables") return reply("/api/tunables", "get", structuredClone(TUNABLES));
      if (path === "/api/players/top") {
        if (ctx.mode === "topbroken") return error(502, "upstream", "Lichess did not answer.");
        const speed = url.searchParams.get("speed") ?? "blitz";
        const rows = (LEADERBOARD[speed] ?? LEADERBOARD["blitz"]!).map(
          ([username, title, rating], i): LeaderboardEntry => ({
            key: `lichess:${username.toLowerCase()}`,
            username,
            title,
            rating,
            progress: Math.round(((i * 7) % 13) - 4),
          }),
        );
        return reply("/api/players/top", "get", rows);
      }
      if ((m = path.match(/^\/api\/players\/([^/]+)\/profile$/))) {
        const key = decodeURIComponent(m[1]!);
        const filter = filterOf(url);
        const quirk = profileQuirk(key);
        if (quirk === "broken") return error(500, "internal", "The profile could not be added up. Try again.");
        const p = build(key, filter);
        const answer = p
          ? reply("/api/players/{key}/profile", "get", p)
          : error(404, "not_found", "Lichess has no such player.");
        // A slow answer, to see the loading state: `lichess:slow…`, or You with `?fixtures=slow`.
        return quirk === "slow" ? new Promise<Response>((done) => setTimeout(() => done(answer), 6000)) : answer;
      }
      if ((m = path.match(/^\/api\/players\/([^/]+)$/))) {
        const p = player(decodeURIComponent(m[1]!));
        return p ? reply("/api/players/{key}", "get", p) : error(404, "not_found", "Lichess has no such player.");
      }
      if (path === "/api/compare") {
        const keys = (url.searchParams.get("players") ?? "you,dameless").split(",").filter(Boolean);
        const filter = filterOf(url);
        if (keys.some((k) => profileQuirk(k) === "broken"))
          return error(500, "internal", "The comparison could not be added up. Try again.");
        const profiles = keys.map((k) => build(k, filter)).filter((p): p is Profile => !!p);
        if (profiles.length !== keys.length) return error(404, "not_found", "Lichess has no such player.");
        return reply("/api/compare", "get", { profiles, leaders: leaders(profiles) });
      }
      if ((m = path.match(/^\/api\/ideas\/(\d+)\/tune$/))) {
        const id = Number(m[1]);
        const found = ctx.ideas.find((i) => i.id === id);
        if (!found || found.kind !== "tune") return error(404, "not_found", "That idea is not a tune.");
        let state = tunes.get(id);
        if (!state) {
          state = tuneState(id);
          tunes.set(id, state);
        }
        followTune(id, state);
        return reply("/api/ideas/{id}/tune", "get", structuredClone(state));
      }
      if (path === "/api/connections") return undefined;
      if (path === "/api/diagnostics")
        return reply("/api/diagnostics", "get", {
          ok: true,
          items: [
            { key: "fastchess", level: "ok", ok: true, title: "fastchess", detail: "fastchess 1.4.0 is installed." },
            {
              key: "book",
              level: "ok",
              ok: true,
              title: "Opening book",
              detail: "UHO_Lichess_4852_v1.epd is installed.",
            },
            { key: "engine", level: "ok", ok: true, title: "Engine checkout", detail: "C:\\dev\\reckless, on main." },
            {
              key: "clang",
              level: "warn",
              ok: true,
              title: "clang",
              detail: "clang is not on the PATH, so builds with Syzygy support cannot be made.",
              action: { label: "Install it", command: "winget install LLVM.LLVM" },
            },
            {
              key: "pgo",
              level: "ok",
              ok: true,
              title: "PGO builds",
              detail: "cargo-pgo 0.3.0 and llvm-profdata: the bot and games against you play profile-guided builds",
            },
            {
              key: "cuda",
              level: "ok",
              ok: true,
              title: "CUDA",
              detail: "CUDA 12.6 is installed.",
            },
            {
              key: "gpu",
              level: variant("nogpu") ? "warn" : "ok",
              ok: true,
              title: "GPU",
              detail: variant("nogpu")
                ? "No GPU was found, so networks cannot be trained here."
                : "NVIDIA GeForce RTX 3080, 10 GB.",
            },
            { key: "disk", level: "ok", ok: true, title: "Disk", detail: "212 GB free on the data drive." },
          ],
        });
      if (path === "/api/harness")
        return reply("/api/harness", "get", {
          checked: !variant("untrusted"),
          current: !variant("untrusted"),
          trusted: !variant("untrusted"),
          passed: variant("untrusted") ? null : true,
          at: variant("untrusted") ? null : minutesAgo(60 * 24 * 3),
          idea: null,
          running: null,
          last: null,
          summary: variant("untrusted")
            ? "The harness has not been checked on this machine."
            : "Checked on this machine 3 days ago: deterministic over 1000 pairs; symmetric over 4000 pairs at +0.4 ± 2.1 Elo.",
        });
    }

    if (method === "POST") {
      if (path === "/api/ideas")
        return reply("/api/ideas", "post", structuredClone(createIdea(body as unknown as NewIdeaRequest)), 201);
      if ((m = path.match(/^\/api\/ideas\/(\d+)\/(stop|resume|promote)$/))) {
        const found = ctx.ideas.find((i) => i.id === Number(m![1]));
        if (!found) return error(404, "not_found", "No such idea.");
        if (m[2] === "promote") return promote(found);
        found.status = m[2] === "stop" ? "stopped" : "running";
        found.verdict = m[2] === "stop" ? "Stopped by you." : null;
        found.ended_at = m[2] === "stop" ? new Date().toISOString() : null;
        emit("idea.upserted", "ideas", structuredClone(found));
        return reply("/api/ideas/{id}/stop", "post", structuredClone(found));
      }
      if (path === "/api/dameless/push")
        return reply("/api/dameless/push", "post", { remote: "origin", head: "9e3a5e41b0c2" });
      if (path === "/api/builds/prune") {
        const dry = body["dry_run"] === true;
        const result = { builds: [3, 4, 5, 6, 7, 8, 9, 10, 11], bytes: 940_000_000, dry_run: dry, in_use: [] };
        if (!dry && status.machine) {
          status.machine.builds = {
            count: status.machine.builds.count - 9,
            bytes: status.machine.builds.bytes - 940_000_000,
          };
        }
        return reply("/api/builds/prune", "post", result);
      }
      if (path === "/api/reviews" && typeof body["player"] === "string") {
        if (status.policy.assist !== "allowed") return error(409, "fair_play", status.policy.reason ?? "");
        const key = body["player"];
        const r = entry(key);
        if (!r) return error(404, "not_found", "No such player.");
        const background = body["background"] === true;
        const all = background && body["limit"] == null;
        const limit = all ? Infinity : Number(body["limit"] ?? 20);
        const todo = Math.min(limit, r.games - r.reviewed);
        const w = startWork(
          all ? `Reviewing all ${todo} of ${key === "you" ? "your" : `${r.name}'s`} games` : `Reviewing ${todo} games`,
          "review",
          key,
          todo,
          () => {
            r.reviewed += todo;
          },
          background ? "background" : "review",
        );
        return reply("/api/reviews", "post", structuredClone(w), 202);
      }
      if ((m = path.match(/^\/api\/players\/([^/]+)\/import$/))) {
        if (status.policy.assist === "off") return error(409, "fair_play", status.policy.reason ?? "");
        const key = decodeURIComponent(m[1]!);
        const r = entry(key);
        if (!r) return error(404, "not_found", "Lichess has no such player.");
        // Your history comes in whole when no `max` is given: about 3,700 games here.
        const max =
          body["max"] != null ? Math.min(Number(body["max"]), 1000) : key === "you" ? Math.max(r.games, 3708) : 50;
        const title = key === "you" ? "Importing your Lichess games" : `Importing ${r.name}'s games`;
        const w = startWork(title, "import", key, max, () => {
          r.games = Math.max(r.games, max);
        });
        return reply("/api/players/{key}/import", "post", structuredClone(w), 202);
      }
      if (path === "/api/connections/you/login") {
        const you = ctx.connections.you;
        setTimeout(() => {
          Object.assign(you, {
            configured: true,
            account: youAccount,
            state: "online",
            you: {
              scopes: ["board:play", "challenge:read", "challenge:write"],
              missing_scopes: [],
              expires_at: minutesAgo(-60 * 24 * 365),
              elsewhere: [],
              seek: null,
              seek_note: null,
            },
          });
          emit("connection.upserted", "connection:you", structuredClone(you));
        }, 600);
        return reply("/api/connections/you/login", "post", {
          url: "/settings?lichess=connected",
          expires_in_seconds: 600,
        });
      }
      if ((m = path.match(/^\/api\/connections\/(you|bot)\/challenges\/([^/]+)\/(accept|decline)$/))) {
        const c = ctx.connections[m[1] as "you" | "bot"];
        const found = c.challenges.find((x) => x.id === m![2]);
        if (!found) return error(404, "not_found", "That challenge is gone.");
        c.challenges = c.challenges.filter((x) => x !== found);
        emit("challenge", `connection:${c.key}`, {
          challenge: structuredClone(found),
          event: m[3] === "accept" ? "accepted" : "declined",
          reason: m[3] === "accept" ? null : "generic",
          detail: null,
        });
        return reply("/api/connections/{key}/challenges/{id}/accept", "post", structuredClone(c));
      }
      if (path === "/api/connections/you/seek") {
        const you = ctx.connections.you;
        if (!you.configured) return error(409, "conflict", "Log in with Lichess first.");
        if (you.you) you.you.seek_note = "Looking for an opponent on Lichess.";
        return connectionsEmit(you);
      }
      if (path === "/api/connections/bot/online") {
        const b = bot();
        if (body["online"] === true && b.bot && !b.bot.settings.version)
          return error(409, "conflict", "Choose the version the bot plays first: champion, or a version such as v14");
        if (b.bot) b.bot.online = body["online"] === true;
        b.state = body["online"] === true ? "online" : "off";
        if (b.bot && body["online"] !== true) b.bot.seek = { mode: "off", pending: null, note: null };
        return connectionsEmit(b);
      }
      if (path === "/api/connections/bot/seek") {
        const b = bot();
        const mode = body["mode"] as "once" | "always" | "off";
        if (b.bot) {
          b.bot.seek = {
            mode,
            pending: null,
            note: mode === "off" ? null : "Looking for a bot to challenge.",
          };
        }
        return connectionsEmit(b);
      }
    }

    if (method === "PUT") {
      if ((m = path.match(/^\/api\/connections\/(you|bot)\/token$/))) {
        const c = ctx.connections[m[1] as "you" | "bot"];
        const token = String(body["token"] ?? "");
        if (token.length < 8) return error(400, "bad_request", "Lichess did not accept that token.");
        c.configured = true;
        c.state = "online";
        c.error = null;
        c.account = m[1] === "you" ? youAccount : (ctx.connections.bot.account ?? botAccount());
        return connectionsEmit(c);
      }
      if (path === "/api/connections/bot/settings") {
        const b = bot();
        const settings = body as unknown as BotSettings;
        if (b.bot) b.bot.settings = settings;
        connectionsEmit(b);
        return reply("/api/connections/bot/settings", "put", settings);
      }
    }

    if (method === "DELETE") {
      if ((m = path.match(/^\/api\/connections\/(you|bot)$/))) {
        const c = ctx.connections[m[1] as "you" | "bot"];
        Object.assign(c, { configured: false, account: null, state: "off", error: null, playing: [], challenges: [] });
        if (c.bot) c.bot.online = false;
        return connectionsEmit(c);
      }
      if ((m = path.match(/^\/api\/connections\/(you|bot)\/challenges\/([^/]+)$/))) {
        const c = ctx.connections[m[1] as "you" | "bot"];
        const found = c.challenges.find((x) => x.id === m![2]);
        if (!found) return error(404, "not_found", "That challenge is gone.");
        c.challenges = c.challenges.filter((x) => x !== found);
        emit("challenge", `connection:${c.key}`, {
          challenge: structuredClone(found),
          event: "canceled",
          reason: null,
          detail: null,
        });
        return reply("/api/connections/{key}/challenges/{id}", "delete", structuredClone(c));
      }
    }
    return undefined;
  };

  function botAccount(): ConnectionStatus["account"] {
    return {
      id: "damelessbot",
      username: "DamelessBot",
      title: "BOT",
      ratings: { blitz: { rating: 2304, games: 512, provisional: false } },
    };
  }
}

function filterOf(url: URL): ProfileFilter {
  const speeds = (url.searchParams.get("speeds") ?? "").split(",").filter(Boolean) as Speed[];
  const rated = url.searchParams.get("rated");
  return {
    speeds,
    rated: rated === "true" ? true : rated === "false" ? false : null,
    since: url.searchParams.get("since"),
    limit: Number(url.searchParams.get("limit") ?? 200),
  };
}
