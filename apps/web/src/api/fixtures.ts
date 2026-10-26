/**
 * A stand-in server, so the app can be built and reviewed before `dameless serve` exists. It answers the routes the
 * stores call and streams events like the real one, at the fetch level: the client swaps its transport and nothing
 * above it knows. Turn it on with `?fixtures` in the address (or `VITE_FIXTURES=1`); `?fixtures=fairplay` makes your
 * live game a Lichess game, so analysis is off.
 *
 * Everything here is invented: the players, the games and the numbers.
 */
import { makeFen } from "chessops/fen";
import { parseSan } from "chessops/san";
import { makeUci } from "chessops/util";
import { playUci, setup, uciOf } from "../board/position";
import { fixtureMode } from "./mode";
import type {
  AppEvent,
  Classification,
  ClassCounts,
  Connections,
  EvalInfo,
  EventKind,
  EventPayloads,
  Game,
  GameId,
  GameSummary,
  Idea,
  Judgement,
  Move,
  Phase,
  Review,
  ReviewedMove,
  Score,
  Seat,
  SeatAction,
  Side,
  SideReview,
  Source,
  Status,
  TimeControl,
  Trial,
  TrialSummary,
  Version,
  Work,
} from "./types";

// ---- Helpers

/** A small seeded random generator (mulberry32), so the fixtures are the same on every load. */
function rng(seed: number) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const NOW = Date.now();
const minutesAgo = (m: number) => new Date(NOW - m * 60_000).toISOString();
const clockTc = (minutes: number, inc: number): TimeControl => ({
  type: "clock",
  initial_ms: minutes * 60_000,
  increment_ms: inc * 1000,
});

const you = (rating: number | null = 1840): Seat => ({
  player: "you",
  name: "dame_g",
  version: null,
  build: null,
  rating,
  title: null,
});
const dameless = (version: number, rating: number | null = null): Seat => ({
  player: "dameless",
  name: "Dameless",
  version,
  build: version + 100,
  rating,
  title: rating ? "BOT" : null,
});
const lichess = (name: string, rating: number, title: string | null = null): Seat => ({
  player: `lichess:${name.toLowerCase()}`,
  name,
  version: null,
  build: null,
  rating,
  title,
});
const stockfish = (level: number): Seat => ({
  player: `stockfish:${level}`,
  name: `Stockfish level ${level}`,
  version: null,
  build: null,
  rating: null,
  title: null,
});

/** The moves of a game: SAN as far as it is legal, then (to `plies`) a seeded walk that likes captures and checks. */
function moves(san: string, plies: number, seed: number): string[] {
  const pos = setup("startpos")!;
  const out: string[] = [];
  for (const token of san.split(/\s+/).filter((t) => t && !/^\d+\./.test(t))) {
    const move = parseSan(pos, token);
    if (!move) break;
    out.push(uciOf(pos, move));
    pos.play(move);
  }
  const random = rng(seed);
  while (out.length < plies && !pos.isEnd()) {
    const legal: string[] = [];
    const sharp: string[] = [];
    for (const [from, dests] of pos.allDests()) {
      for (const to of dests) {
        const uci = makeUci({ from, to });
        legal.push(uci);
        if (pos.board.get(to)) sharp.push(uci);
      }
    }
    const pool = sharp.length && random() < 0.45 ? sharp : legal;
    const uci = pool[Math.floor(random() * pool.length)]!;
    const played = playUci(pos, uci);
    if (!played) break;
    out.push(played.uci);
  }
  return out;
}

/** A White-POV evaluation walk for a game, with a few swings, ending where the result says. */
function evals(n: number, seed: number, drift = 0): number[] {
  const random = rng(seed);
  const out: number[] = [];
  let cp = 20;
  for (let i = 0; i < n; i++) {
    const swing = random() < 0.08 ? (random() < 0.5 ? -1 : 1) * (150 + random() * 250) : 0;
    cp = Math.max(-1500, Math.min(1500, cp + (random() - 0.5) * 60 + swing + drift));
    out.push(Math.round(cp));
  }
  return out;
}

/** Plays UCI moves into `Move` records with SAN, clocks and, for engine seats, their scores. */
function record(ucis: string[], opts: { tc?: TimeControl; engine?: Side[]; seed: number; drift?: number }): Move[] {
  const pos = setup("startpos")!;
  const scores = evals(ucis.length, opts.seed, opts.drift);
  const random = rng(opts.seed + 7);
  const clocks = {
    white: opts.tc?.type === "clock" ? opts.tc.initial_ms : 0,
    black: opts.tc?.type === "clock" ? opts.tc.initial_ms : 0,
  };
  const out: Move[] = [];
  ucis.forEach((uci, i) => {
    const side: Side = pos.turn;
    const played = playUci(pos, uci);
    if (!played) return;
    const think = Math.round(800 + random() * 9000);
    let clock: number | null = null;
    if (opts.tc?.type === "clock") {
      clocks[side] = Math.max(1000, clocks[side] - think + opts.tc.increment_ms);
      clock = clocks[side];
    }
    const engine = opts.engine?.includes(side);
    out.push({
      uci: played.uci,
      san: played.san,
      clock_ms: clock,
      think_ms: think,
      eval: engine ? { cp: scores[i]! } : null,
      depth: engine ? 14 + Math.floor(random() * 8) : null,
      nodes: engine ? Math.round(200_000 + random() * 2_000_000) : null,
    });
  });
  return out;
}

const OPERA = `1. e4 e5 2. Nf3 d6 3. d4 Bg4 4. dxe5 Bxf3 5. Qxf3 dxe5 6. Bc4 Nf6 7. Qb3 Qe7 8. Nc3 c6 9. Bg5 b5
  10. Nxb5 cxb5 11. Bxb5+ Nbd7 12. O-O-O Rd8 13. Rxd7 Rxd7 14. Rd1 Qe6 15. Bxd7+ Nxd7 16. Qb8+ Nxb8 17. Rd8#`;
const ITALIAN = "1. e4 e5 2. Nf3 Nc6 3. Bc4 Bc5 4. c3 Nf6 5. d3 d6 6. O-O a6 7. Re1 Ba7 8. Bb3 O-O 9. h3 h6";
const SICILIAN = "1. e4 c5 2. Nf3 d6 3. d4 cxd4 4. Nxd4 Nf6 5. Nc3 a6 6. Be3 e5 7. Nb3 Be6 8. f3 Be7 9. Qd2 O-O";
const QGD = "1. d4 d5 2. c4 e6 3. Nc3 Nf6 4. Bg5 Be7 5. e3 O-O 6. Nf3 h6 7. Bh4 b6 8. cxd5 Nxd5 9. Bxe7 Qxe7";
const LONDON = "1. d4 Nf6 2. Bf4 d5 3. e3 c5 4. c3 Nc6 5. Nd2 e6 6. Ngf3 Bd6 7. Bg3 O-O 8. Bd3 b6 9. Ne5 Bb7";
const CARO = "1. e4 c6 2. d4 d5 3. e5 Bf5 4. Nf3 e6 5. Be2 Nd7 6. O-O Ne7 7. Nbd2 h6 8. Nb3 g5 9. Ne1 Bg7";

// ---- Versions

const versions: Version[] = [];
for (let n = 1; n <= 14; n++) {
  versions.push({
    id: n,
    name: `v${n}`,
    commit: `${(0xa1b2c3d + n * 7919).toString(16)}e0f${n}`.padEnd(40, "0"),
    branch: n === 14 ? "main" : null,
    line: { type: "champion", number: n },
    parent: n > 1 ? n - 1 : null,
    bench: 1_480_000 + n * 13_771,
    elo: n > 1 ? { elo: 2 + ((n * 37) % 9) / 2, low: 0.4, high: 7.5 } : null,
    created_at: minutesAgo((15 - n) * 60 * 24 * 4),
  });
}
versions.push(
  {
    id: 20,
    name: "deeper-lmr",
    commit: "d33p3r1mr".padEnd(40, "0"),
    branch: "deeper-lmr",
    line: { type: "idea", idea: 1 },
    parent: 14,
    bench: 1_702_118,
    elo: null,
    created_at: minutesAgo(95),
  },
  {
    id: 21,
    name: "qsearch-evasions",
    commit: "9e3a5e".padEnd(40, "0"),
    branch: "qsearch-evasions",
    line: { type: "idea", idea: 2 },
    parent: 13,
    bench: 1_655_302,
    elo: { elo: 0.8, low: -1.3, high: 2.9 },
    created_at: minutesAgo(60 * 30),
  },
  {
    id: 30,
    name: "reckless/main",
    commit: "7ec4e55".padEnd(40, "0"),
    branch: "upstream/main",
    line: { type: "upstream" },
    parent: null,
    bench: 1_690_455,
    elo: null,
    created_at: minutesAgo(60 * 50),
  },
);

// ---- Ideas and trials

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

const stc: TimeControl = { type: "clock", initial_ms: 8000, increment_ms: 80 };
const vstc: TimeControl = { type: "clock", initial_ms: 4000, increment_ms: 40 };
const scaling = {
  copies: 14,
  dev_nps: 482_000,
  base_nps: 478_000,
  measured_nps: 478_000,
  reference_nps: 660_000,
  scale: 1.38,
};

const trials: Trial[] = [
  trial({
    id: 1,
    idea: 1,
    stage: "fingerprint",
    status: "passed",
    detail: "Bench changed: 1,702,118 nodes (v14 has 1,672,794).",
    started_at: minutesAgo(95),
    ended_at: minutesAgo(93),
  }),
  trial({
    id: 2,
    idea: 1,
    stage: "screen",
    status: "passed",
    preset: "vstc",
    time: vstc,
    played: { type: "clock", initial_ms: 5520, increment_ms: 55 },
    scaling,
    bounds: [-5, 0],
    penta: [21, 160, 402, 171, 26],
    wdl: [1310, 1940, 1350],
    llr: { value: 2.96, lower: -2.94, upper: 2.94 },
    elo: { elo: 3.0, low: -2.1, high: 8.1 },
    nelo: { elo: 4.4, low: -3.0, high: 11.8 },
    detail: "Passed the screen after 780 pairs: +3.0 ± 5.1 Elo.",
    started_at: minutesAgo(93),
    ended_at: minutesAgo(61),
  }),
  trial({
    id: 3,
    idea: 1,
    stage: "short",
    status: "running",
    preset: "stc",
    time: stc,
    played: { type: "clock", initial_ms: 11_040, increment_ms: 110 },
    scaling,
    bounds: [0, 3],
    penta: [9, 88, 214, 92, 9],
    wdl: [702, 1003, 707],
    llr: { value: 0.61, lower: -2.94, upper: 2.94 },
    elo: { elo: 0.7, low: -5.2, high: 6.6 },
    nelo: { elo: 1.1, low: -8.0, high: 10.2 },
    detail: "412 pairs so far: +0.7 ± 5.9 Elo.",
    started_at: minutesAgo(61),
  }),
  trial({
    id: 4,
    idea: 2,
    stage: "fingerprint",
    status: "passed",
    detail: "Bench changed.",
    started_at: minutesAgo(60 * 30),
    ended_at: minutesAgo(60 * 30 - 2),
  }),
  trial({
    id: 5,
    idea: 2,
    stage: "screen",
    status: "passed",
    preset: "vstc",
    time: vstc,
    bounds: [-5, 0],
    penta: [30, 190, 455, 186, 29],
    wdl: [1510, 2280, 1500],
    llr: { value: 2.95, lower: -2.94, upper: 2.94 },
    elo: { elo: -0.5, low: -5.0, high: 4.0 },
    detail: "Passed the screen after 890 pairs.",
    started_at: minutesAgo(60 * 30),
    ended_at: minutesAgo(60 * 29),
  }),
  trial({
    id: 6,
    idea: 2,
    stage: "short",
    status: "passed",
    preset: "stc",
    time: stc,
    bounds: [-2.75, 0.25],
    penta: [80, 640, 1622, 650, 84],
    wdl: [5320, 7890, 5420],
    llr: { value: 2.97, lower: -2.94, upper: 2.94 },
    elo: { elo: 0.8, low: -1.3, high: 2.9 },
    nelo: { elo: 1.3, low: -2.0, high: 4.6 },
    detail: "No worse after 3,076 pairs: +0.8 ± 2.1 Elo.",
    started_at: minutesAgo(60 * 29),
    ended_at: minutesAgo(60 * 22),
  }),
  trial({
    id: 7,
    idea: 3,
    stage: "fingerprint",
    status: "passed",
    detail: "Bench changed.",
    started_at: minutesAgo(60 * 50),
    ended_at: minutesAgo(60 * 50 - 2),
  }),
  trial({
    id: 8,
    idea: 3,
    stage: "screen",
    status: "failed",
    preset: "vstc",
    time: vstc,
    bounds: [-5, 0],
    penta: [44, 190, 360, 140, 22],
    wdl: [1420, 1690, 1210],
    llr: { value: -2.95, lower: -2.94, upper: 2.94 },
    elo: { elo: -9.6, low: -15.0, high: -4.2 },
    detail: "Lost clearly at the screen: −9.6 ± 5.4 Elo.",
    started_at: minutesAgo(60 * 50),
    ended_at: minutesAgo(60 * 49),
  }),
];

const summaryOf = (t: Trial): TrialSummary => ({
  id: t.id,
  stage: t.stage,
  status: t.status,
  time: t.time,
  penta: t.penta,
  llr: t.llr,
  elo: t.elo,
  detail: t.detail,
});

const ideas: Idea[] = [
  {
    id: 1,
    title: "Deeper LMR at high depth",
    hypothesis:
      "Late moves at depth 12 and more are rarely best; reducing them one ply more saves time for the main line.",
    kind: "gain",
    author: "claude",
    branch: "deeper-lmr",
    base: 14,
    version: 20,
    status: "running",
    verdict: null,
    trials: [],
    tags: ["search-oct"],
    notes: "The reduction table already grows with depth; this adds one ply past depth 12 for quiet late moves.",
    created_at: minutesAgo(96),
    ended_at: null,
  },
  {
    id: 2,
    title: "Check evasions in quiescence",
    hypothesis: "Searching every evasion when in check in qsearch is simpler and should be no weaker.",
    kind: "neutral",
    author: "you",
    branch: "qsearch-evasions",
    base: 13,
    version: 21,
    status: "kept",
    verdict: "Kept: +0.8 ± 2.1 Elo at short games, no worse.",
    trials: [],
    tags: [],
    notes: null,
    created_at: minutesAgo(60 * 30),
    ended_at: minutesAgo(60 * 22),
  },
  {
    id: 3,
    title: "Wider aspiration windows",
    hypothesis: "Starting the window at 20 cp instead of 12 re-searches less often.",
    kind: "gain",
    author: "claude",
    branch: "wide-aspiration",
    base: 13,
    version: null,
    status: "dropped",
    verdict: "Dropped at the screen: −9.6 ± 5.4 Elo.",
    trials: [],
    tags: ["search-oct"],
    notes: null,
    created_at: minutesAgo(60 * 50),
    ended_at: minutesAgo(60 * 49),
  },
];
for (const idea of ideas) idea.trials = trials.filter((t) => t.idea === idea.id).map(summaryOf);

// ---- Games

interface Stored {
  game: Game;
  review: Review | null;
}
const games = new Map<GameId, Stored>();

function summary(
  s: Partial<GameSummary> & Pick<GameSummary, "id" | "white" | "black" | "source" | "time">,
): GameSummary {
  return {
    status: "finished",
    outcome: null,
    ending: null,
    opening: null,
    plies: 0,
    started_at: minutesAgo(60),
    ended_at: null,
    you: null,
    accuracy: null,
    url: null,
    ...s,
  };
}

function add(s: GameSummary, ms: Move[], extra: Partial<Game> = {}) {
  s.plies = ms.length;
  games.set(s.id, {
    game: {
      summary: s,
      start_fen: null,
      moves: ms,
      clocks: null,
      offers: { draw: null, takeback: null },
      actions: [],
      ...extra,
    },
    review: null,
  });
}

const fairplay = fixtureMode() === "fairplay";

// Your live game: against Dameless here, or (fair-play mode) on Lichess.
{
  const tc = clockTc(5, 3);
  const ms = record(moves(ITALIAN, 18, 11), { tc, engine: fairplay ? [] : ["black"], seed: 11 });
  const source: Source = fairplay
    ? { type: "lichess", connection: "you", id: "aB3dE6gH", rated: true, speed: "blitz" }
    : { type: "local" };
  add(
    summary({
      id: 1,
      white: you(),
      black: fairplay ? lichess("cutemouse83", 1871) : dameless(14),
      source,
      time: tc,
      status: "live",
      opening: { eco: "C54", name: "Italian Game: Giuoco Pianissimo", ply: 10 },
      started_at: minutesAgo(6),
      you: "white",
      url: fairplay ? "https://lichess.org/aB3dE6gH" : null,
    }),
    ms,
    { clocks: { white: 212_400, black: 238_100 }, actions: ["move", "offer_draw", "takeback", "resign"] },
  );
}

// Dameless on Lichess, live: played out move by move from a script.
const botScript = moves(SICILIAN, 70, 22);
{
  const tc = clockTc(3, 2);
  add(
    summary({
      id: 2,
      white: lichess("Alpha-Bot", 2251, "BOT"),
      black: dameless(14, 2304),
      source: { type: "lichess", connection: "bot", id: "Qx7pLm2R", rated: true, speed: "blitz" },
      time: tc,
      status: "live",
      opening: { eco: "B90", name: "Sicilian Defense: Najdorf Variation, English Attack", ply: 12 },
      started_at: minutesAgo(2),
      url: "https://lichess.org/Qx7pLm2R",
    }),
    record(botScript.slice(0, 16), { tc, engine: ["black"], seed: 22 }),
    { clocks: { white: 162_000, black: 171_500 } },
  );
}

const finished: [
  Partial<GameSummary> & Pick<GameSummary, "id" | "white" | "black" | "source" | "time">,
  string,
  number,
  number,
][] = [
  [
    {
      id: 3,
      white: you(),
      black: dameless(13),
      source: { type: "local" },
      time: clockTc(10, 5),
      outcome: "white_wins",
      ending: "checkmate",
      opening: { eco: "C41", name: "Philidor Defense", ply: 6 },
      started_at: minutesAgo(60 * 20),
      ended_at: minutesAgo(60 * 20 - 14),
      you: "white",
    },
    OPERA,
    33,
    3,
  ],
  [
    {
      id: 4,
      white: lichess("cutemouse83", 1862),
      black: you(1846),
      source: { type: "lichess", connection: "you", id: "k2LmN8pQ", rated: true, speed: "rapid" },
      time: clockTc(10, 5),
      outcome: "white_wins",
      ending: "resignation",
      opening: { eco: "C02", name: "French Defense: Advance Variation", ply: 5 },
      started_at: minutesAgo(60 * 26),
      ended_at: minutesAgo(60 * 26 - 21),
      you: "black",
      url: "https://lichess.org/k2LmN8pQ",
    },
    CARO,
    61,
    4,
  ],
  [
    {
      id: 5,
      white: stockfish(5),
      black: you(1846),
      source: { type: "lichess", connection: "you", id: "s7FiSh55", rated: false, speed: "rapid" },
      time: clockTc(15, 10),
      outcome: "draw",
      ending: "repetition",
      opening: { eco: "D53", name: "Queen's Gambit Declined", ply: 8 },
      started_at: minutesAgo(60 * 48),
      ended_at: minutesAgo(60 * 48 - 33),
      you: "black",
      url: "https://lichess.org/s7FiSh55",
    },
    QGD,
    54,
    5,
  ],
  [
    {
      id: 6,
      white: dameless(14, 2298),
      black: lichess("Bot-Sam", 2190, "BOT"),
      source: { type: "lichess", connection: "bot", id: "bS4mPl3x", rated: true, speed: "blitz" },
      time: clockTc(3, 0),
      outcome: "white_wins",
      ending: "timeout",
      opening: { eco: "D02", name: "London System", ply: 6 },
      started_at: minutesAgo(40),
      ended_at: minutesAgo(33),
      url: "https://lichess.org/bS4mPl3x",
    },
    LONDON,
    88,
    6,
  ],
  [
    {
      id: 7,
      white: you(1790),
      black: lichess("knightowl", 1802),
      source: { type: "import", owner: "you", id: "iMp0rt7d", rated: true, speed: "blitz" },
      time: clockTc(5, 0),
      outcome: "black_wins",
      ending: "timeout",
      opening: { eco: "B12", name: "Caro-Kann Defense: Advance Variation", ply: 6 },
      started_at: minutesAgo(60 * 24 * 21),
      ended_at: minutesAgo(60 * 24 * 21 - 11),
      you: "white",
      url: "https://lichess.org/iMp0rt7d",
    },
    CARO,
    72,
    7,
  ],
];
for (const [s, san, plies, seed] of finished) {
  const engine: Side[] = [];
  if (s.white.player === "dameless") engine.push("white");
  if (s.black.player === "dameless") engine.push("black");
  add(
    summary(s),
    record(moves(san, plies, seed), {
      tc: s.time,
      engine,
      seed,
      drift: s.outcome === "white_wins" ? 6 : s.outcome === "black_wins" ? -6 : 0,
    }),
  );
}

// A trial's games: the version under test against the champion, colours alternating by pair.
function trialGames(trialId: number, first: GameId, count: number, dev: number, base: number, minutes: number) {
  const outcomes = ["white_wins", "draw", "black_wins", "draw", "white_wins", "draw"] as const;
  for (let i = 0; i < count; i++) {
    const devWhite = i % 2 === 0;
    const openings = [ITALIAN, SICILIAN, QGD, LONDON, CARO];
    const san = openings[Math.floor(i / 2) % openings.length]!;
    const tc = { type: "clock", initial_ms: 11_040, increment_ms: 110 } as const;
    const outcome = outcomes[i % outcomes.length]!;
    add(
      summary({
        id: first + i,
        white: dameless(devWhite ? dev : base),
        black: dameless(devWhite ? base : dev),
        source: { type: "trial", trial: trialId, index: i },
        time: tc,
        outcome,
        ending: outcome === "draw" ? "adjudication" : "adjudication",
        started_at: minutesAgo(minutes - i),
        ended_at: minutesAgo(minutes - i - 1),
      }),
      record(moves(san, 60 + ((i * 17) % 50), 100 + first + i), {
        tc,
        engine: ["white", "black"],
        seed: 100 + first + i,
        drift: outcome === "white_wins" ? 8 : outcome === "black_wins" ? -8 : 0,
      }),
    );
  }
}
trialGames(3, 101, 8, 20, 14, 40);
trialGames(6, 201, 6, 21, 13, 60 * 25);

// ---- Reviews

const winPct = (cp: number) => 50 + 50 * (2 / (1 + Math.exp(-0.00368208 * Math.max(-1000, Math.min(1000, cp)))) - 1);
const moveAccuracy = (drop: number) =>
  drop <= 0 ? 100 : Math.max(0, Math.min(100, 103.1668 * Math.exp(-0.04354 * drop) - 3.1669 + 1));

function emptySide(): SideReview {
  const counts = Object.fromEntries(
    ["brilliant", "great", "best", "excellent", "good", "book", "inaccuracy", "mistake", "blunder", "miss"].map((c) => [
      c,
      0,
    ]),
  ) as ClassCounts;
  const phase = () => ({ moves: 0, accuracy: null, acpl: null, timed_moves: 0, time_ms: 0 });
  return {
    moves: 0,
    accuracy: null,
    acpl: null,
    counts,
    judgements: { inaccuracy: 0, mistake: 0, blunder: 0 },
    phases: { opening: phase(), middlegame: phase(), endgame: phase() },
    time_trouble_blunders: 0,
  };
}

/** A review in the real one's shape, from a seeded evaluation walk: judged by Lichess's win% thresholds. */
function synthReview(stored: Stored): Review {
  const g = stored.game;
  const id = g.summary.id;
  const pos = setup(g.start_fen ?? "startpos")!;
  const drift = g.summary.outcome === "white_wins" ? 9 : g.summary.outcome === "black_wins" ? -9 : 0;
  const walk = [20, ...evals(g.moves.length, id * 31, drift)];
  const random = rng(id * 13);
  const sides = { white: emptySide(), black: emptySide() };
  const acc: Record<Side, number[]> = { white: [], black: [] };
  const loss: Record<Side, number[]> = { white: [], black: [] };
  const plies: ReviewedMove[] = [];
  const bookPly = g.summary.opening?.ply ?? 0;
  g.moves.forEach((m, i) => {
    const side: Side = pos.turn;
    const sign = side === "white" ? 1 : -1;
    const before = walk[i]!;
    const after = walk[i + 1]!;
    const winBefore = winPct(before * sign);
    const winAfter = winPct(after * sign);
    const drop = Math.max(0, winBefore - winAfter);
    const legal: string[] = [];
    for (const [from, dests] of pos.allDests()) for (const to of dests) legal.push(makeUci({ from, to }));
    const best = drop < 1 ? m.uci : (legal.find((u) => u !== m.uci) ?? m.uci);
    const bestPos = pos.clone();
    const bestSan = playUci(bestPos, best)?.san ?? m.san;
    const pv = [best];
    for (let k = 0; k < 4; k++) {
      const replies: string[] = [];
      for (const [from, dests] of bestPos.allDests()) for (const to of dests) replies.push(makeUci({ from, to }));
      const reply = replies[Math.floor(random() * replies.length)];
      if (!reply || !playUci(bestPos, reply)) break;
      pv.push(reply);
    }
    let judgement: Judgement | null = null;
    if (best !== m.uci) judgement = drop >= 15 ? "blunder" : drop >= 10 ? "mistake" : drop >= 5 ? "inaccuracy" : null;
    let classification: Classification =
      i + 1 <= bookPly ? "book" : (judgement ?? (best === m.uci ? "best" : drop < 2 ? "excellent" : "good"));
    if (classification === "best" && random() < 0.05) classification = "great";
    if (classification === "best" && random() < 0.02 && winBefore < 85) classification = "brilliant";
    const accuracy = moveAccuracy(drop);
    const phase: Phase = i < 20 ? "opening" : i < 60 ? "middlegame" : "endgame";
    const s = sides[side];
    s.moves++;
    s.counts[classification]++;
    if (judgement) s.judgements[judgement]++;
    s.phases[phase].moves++;
    acc[side].push(accuracy);
    loss[side].push(Math.max(0, (before - after) * sign));
    const score = (cp: number): Score => ({ cp });
    plies.push({
      ply: i + 1,
      side,
      uci: m.uci,
      san: m.san,
      eval_before: score(before),
      eval_after: score(after),
      best_uci: best,
      best_san: bestSan,
      best_pv: pv,
      second_uci: legal[0] ?? null,
      eval_second: score(before - 40 * sign),
      win_before: winBefore,
      win_after: winAfter,
      accuracy,
      cp_loss: Math.round(Math.max(0, (before - after) * sign)),
      classification,
      judgement,
      phase,
      sacrifice: classification === "brilliant" ? 3 : 0,
      clock_before_ms: m.clock_ms,
      depth: 14,
    });
    playUci(pos, m.uci);
  });
  for (const side of ["white", "black"] as const) {
    const a = acc[side];
    sides[side].accuracy = a.length ? a.reduce((x, y) => x + y, 0) / a.length : null;
    sides[side].acpl = loss[side].length ? Math.round(loss[side].reduce((x, y) => x + y, 0) / loss[side].length) : null;
  }
  return {
    id: id + 1000,
    game: id,
    version: 14,
    nodes: 200_000,
    summary: {
      version: 14,
      nodes: 200_000,
      white: sides.white,
      black: sides.black,
      middlegame_ply: 21,
      endgame_ply: 61,
    },
    plies,
    created_at: minutesAgo(10),
  };
}

function reviewGame(id: GameId) {
  const stored = games.get(id);
  if (!stored) return;
  stored.review = synthReview(stored);
  const { white, black } = stored.review.summary;
  stored.game.summary.accuracy = { white: Math.round(white.accuracy ?? 0), black: Math.round(black.accuracy ?? 0) };
}
for (const id of [3, 4, 6, 101, 102]) reviewGame(id);

// ---- Work, connections, policy

const work: Work[] = [
  {
    id: 1,
    kind: "trial",
    status: "running",
    priority: "idea",
    title: "Testing deeper-lmr · short games",
    subject: { type: "trial", id: 3 },
    cores: 14,
    progress: { done: 412, total: null, unit: "pairs", eta_seconds: 2400, text: "412 pairs · +0.7 ± 5.9 Elo" },
    paused_for: null,
    error: null,
    created_at: minutesAgo(61),
    started_at: minutesAgo(61),
    ended_at: null,
  },
  {
    id: 2,
    kind: "import",
    status: "queued",
    priority: "interactive",
    title: "Importing cutemouse83's games",
    subject: { type: "player", key: "lichess:cutemouse83" },
    cores: 0,
    progress: null,
    paused_for: null,
    error: null,
    created_at: minutesAgo(3),
    started_at: null,
    ended_at: null,
  },
  {
    id: 3,
    kind: "review",
    status: "done",
    priority: "review",
    title: "Reviewing your last 3 games",
    subject: { type: "player", key: "you" },
    cores: 0,
    progress: { done: 3, total: 3, unit: "games", eta_seconds: null, text: null },
    paused_for: null,
    error: null,
    created_at: minutesAgo(14),
    started_at: minutesAgo(14),
    ended_at: minutesAgo(12),
  },
  {
    id: 4,
    kind: "build",
    status: "done",
    priority: "interactive",
    title: "Building deeper-lmr",
    subject: { type: "version", id: 20 },
    cores: 0,
    progress: null,
    paused_for: null,
    error: null,
    created_at: minutesAgo(96),
    started_at: minutesAgo(96),
    ended_at: minutesAgo(95),
  },
];

const connections: Connections = {
  you: {
    key: "you",
    configured: true,
    account: {
      id: "dame_g",
      username: "dame_g",
      title: null,
      ratings: {
        blitz: { rating: 1840, games: 812, provisional: false },
        rapid: { rating: 1846, games: 120, provisional: false },
      },
    },
    state: "online",
    error: null,
    playing: fairplay ? [1] : [],
  },
  bot: {
    key: "bot",
    configured: true,
    account: {
      id: "damelessbot",
      username: "DamelessBot",
      title: "BOT",
      ratings: {
        bullet: { rating: 2281, games: 340, provisional: false },
        blitz: { rating: 2304, games: 512, provisional: false },
      },
    },
    state: "online",
    error: null,
    playing: [2],
    online: true,
  },
};

const status: Status = {
  version: "0.1.0-fixtures",
  cores: 15,
  policy: fairplay
    ? { assist: "off", reason: "Analysis is off while you're playing on Lichess" }
    : { assist: "allowed", reason: null },
};

// ---- Events

interface Subscriber {
  topics: Set<string>;
  send: (event: AppEvent) => void;
}
const subscribers = new Set<Subscriber>();
const backlog: AppEvent[] = [];
let lastId = 0;

function emit<K extends EventKind>(kind: K, topic: string, data: EventPayloads[K]) {
  const event = { id: ++lastId, topic, kind, data } as AppEvent;
  backlog.push(event);
  if (backlog.length > 500) backlog.shift();
  for (const s of subscribers) if (s.topics.has(topic)) s.send(event);
}

function streamResponse(url: URL, signal: AbortSignal | null | undefined): Response {
  const topics = new Set((url.searchParams.get("topics") ?? "").split(",").filter(Boolean));
  const since = Number(url.searchParams.get("since") ?? NaN);
  const encoder = new TextEncoder();
  let sub: Subscriber | undefined;
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      const send = (e: AppEvent) => controller.enqueue(encoder.encode(`id: ${e.id}\ndata: ${JSON.stringify(e)}\n\n`));
      if (Number.isFinite(since)) for (const e of backlog) if (e.id > since && topics.has(e.topic)) send(e);
      sub = { topics, send };
      subscribers.add(sub);
      signal?.addEventListener("abort", () => {
        if (sub) subscribers.delete(sub);
        try {
          controller.close();
        } catch {}
      });
    },
    cancel() {
      if (sub) subscribers.delete(sub);
    },
  });
  return new Response(body, { headers: { "Content-Type": "text/event-stream" } });
}

// ---- Live behaviour

const other = (side: Side): Side => (side === "white" ? "black" : "white");

const fenOf = (g: Game) => {
  const pos = setup(g.start_fen ?? "startpos")!;
  for (const m of g.moves) playUci(pos, m.uci);
  return pos;
};

/** Plays a move in a stored game and tells the stream, charging the mover's clock. */
function playMove(g: Game, uci: string, think: number, score: Score | null) {
  const pos = fenOf(g);
  const side: Side = pos.turn;
  const played = playUci(pos, uci);
  if (!played) return false;
  let clockMs: number | null = null;
  if (g.clocks && g.summary.time.type === "clock") {
    g.clocks[side] = Math.max(0, g.clocks[side] - think + g.summary.time.increment_ms);
    clockMs = g.clocks[side];
  }
  const move: Move = {
    uci: played.uci,
    san: played.san,
    clock_ms: clockMs,
    think_ms: think,
    eval: score,
    depth: score ? 18 : null,
    nodes: score ? 1_200_000 : null,
  };
  g.moves.push(move);
  g.summary.plies = g.moves.length;
  emit("game.moved", `game:${g.summary.id}`, {
    id: g.summary.id,
    ply: g.moves.length,
    move,
    clocks: g.clocks ? { ...g.clocks } : null,
    fen: makeFen(pos.toSetup()),
  });
  if (pos.isEnd())
    finish(
      g,
      pos.isCheckmate() ? (side === "white" ? "white_wins" : "black_wins") : "draw",
      pos.isCheckmate() ? "checkmate" : "stalemate",
    );
  return true;
}

function finish(g: Game, outcome: GameSummary["outcome"], ending: GameSummary["ending"]) {
  g.summary.status = "finished";
  g.summary.outcome = outcome;
  g.summary.ending = ending;
  g.summary.ended_at = new Date().toISOString();
  g.actions = [];
  g.clocks = null;
  emit("game.upserted", "games", { ...g.summary });
  emit("game.changed", `game:${g.summary.id}`, {
    id: g.summary.id,
    offers: g.offers,
    actions: [],
    clocks: null,
    opponent_gone: null,
  });
}

/** Legal moves of a position, captures first, as UCI. */
function choices(g: Game): string[] {
  const pos = fenOf(g);
  const caps: string[] = [];
  const quiet: string[] = [];
  for (const [from, dests] of pos.allDests())
    for (const to of dests) (pos.board.get(to) ? caps : quiet).push(makeUci({ from, to }));
  return [...caps, ...quiet];
}

/** Streams an engine's thinking for a while, then plays its move. */
function think(g: Game, side: Side, ms: number, then: () => void) {
  const id = g.summary.id;
  const random = rng(g.moves.length * 97 + id);
  const pv = choices(g).slice(0, 1);
  let depth = 6;
  const base = (g.moves.at(-1)?.eval as { cp: number } | null)?.cp ?? 20;
  const timer = setInterval(() => {
    depth++;
    emit("game.thinking", `game:${id}`, {
      id,
      ply: g.moves.length + 1,
      side,
      depth,
      score: { cp: Math.round(base + (random() - 0.5) * 40) },
      nodes: depth * 180_000,
      nps: 1_150_000,
      pv: pv.length ? [...pv, ...choices(g).slice(1, 4)] : [],
    });
  }, 250);
  setTimeout(() => {
    clearInterval(timer);
    then();
  }, ms);
}

function botLoop() {
  const g = games.get(2)?.game;
  if (!g || g.summary.status !== "live") return;
  const next = botScript[g.moves.length];
  if (!next) {
    finish(g, "black_wins", "resignation");
    return;
  }
  const side: Side = g.moves.length % 2 === 0 ? "white" : "black";
  const engine = side === "black";
  const delay = 1500 + (g.moves.length % 5) * 600;
  const go = () => {
    const base = (g.moves.at(-1)?.eval as { cp: number } | null)?.cp ?? 0;
    playMove(g, next, delay, engine ? { cp: base - 15 + (g.moves.length % 7) * 6 } : null);
    setTimeout(botLoop, 400);
  };
  if (engine) think(g, side, delay, go);
  else setTimeout(go, delay);
}

function trialLoop() {
  const t = trials.find((x) => x.id === 3)!;
  if (t.status !== "running") return;
  const w = work.find((x) => x.id === 1)!;
  if (w.status === "running") {
    const r = rng(t.penta.reduce((a, b) => a + b, 0));
    const k = Math.floor(r() * 5);
    t.penta[k] = (t.penta[k] ?? 0) + 1;
    const pairs = t.penta.reduce((a, b) => a + b, 0);
    t.llr = { value: Math.round((t.llr!.value + (r() - 0.45) * 0.05) * 100) / 100, lower: -2.94, upper: 2.94 };
    t.detail = `${pairs} pairs so far: ${t.elo!.elo >= 0 ? "+" : "−"}${Math.abs(t.elo!.elo).toFixed(1)} ± 5.9 Elo.`;
    emit("trial.progress", "ideas", { id: 3, penta: t.penta, wdl: t.wdl, llr: t.llr, elo: t.elo });
    w.progress = {
      done: pairs,
      total: null,
      unit: "pairs",
      eta_seconds: Math.max(60, (w.progress?.eta_seconds ?? 2400) - 3),
      text: `${pairs} pairs · +0.7 ± 5.9 Elo`,
    };
    emit("work.upserted", "work", { ...w });
  }
  setTimeout(trialLoop, 3000);
}

let started = false;
function startLive() {
  if (started) return;
  started = true;
  setTimeout(botLoop, 1500);
  setTimeout(trialLoop, 3000);
}

const sessions = new Map<string, ReturnType<typeof setInterval>>();

function startEval(body: { fen: string; moves: string[]; multipv?: number }): string {
  const session = `fx${Math.random().toString(36).slice(2, 8)}`;
  const pos = setup(body.fen)!;
  for (const m of body.moves) playUci(pos, m);
  const fen = makeFen(pos.toSetup());
  const seed = [...fen].reduce((a, c) => (a * 31 + c.charCodeAt(0)) >>> 0, 7);
  const random = rng(seed);
  const base = Math.round((random() - 0.5) * 120);
  const legal: string[] = [];
  for (const [from, dests] of pos.allDests()) for (const to of dests) legal.push(makeUci({ from, to }));
  const lines = Math.min(body.multipv ?? 3, legal.length);
  const pvs = Array.from({ length: lines }, (_, i) => {
    const p = pos.clone();
    const first = legal[Math.floor(random() * legal.length)]!;
    const pv = [first];
    playUci(p, first);
    for (let k = 0; k < 7; k++) {
      const opts: string[] = [];
      for (const [from, dests] of p.allDests()) for (const to of dests) opts.push(makeUci({ from, to }));
      const reply = opts[Math.floor(random() * opts.length)];
      if (!reply) break;
      playUci(p, reply);
      pv.push(reply);
    }
    return { pv, offset: i * 18 };
  });
  let depth = 4;
  const timer = setInterval(() => {
    depth++;
    const info: EvalInfo = {
      session,
      depth,
      nodes: depth * depth * 40_000,
      nps: 1_480_000,
      lines: pvs.map((l) => ({ score: { cp: base - l.offset + Math.round((random() - 0.5) * 8) }, pv: l.pv })),
    };
    emit("eval.info", `eval:${session}`, info);
    if (depth >= 32) clearInterval(timer);
  }, 280);
  sessions.set(session, timer);
  return session;
}

function queueReview(ids: GameId[]) {
  const id = Math.max(...work.map((w) => w.id)) + 1;
  const w: Work = {
    id,
    kind: "review",
    status: "running",
    priority: "review",
    title: ids.length === 1 ? "Reviewing a game" : `Reviewing ${ids.length} games`,
    subject: { type: "game", id: ids[0]! },
    cores: 2,
    progress: { done: 0, total: ids.length, unit: "games", eta_seconds: 4 * ids.length, text: null },
    paused_for: null,
    error: null,
    created_at: new Date().toISOString(),
    started_at: new Date().toISOString(),
    ended_at: null,
  };
  work.push(w);
  emit("work.upserted", "work", { ...w });
  let done = 0;
  const step = () => {
    const game = ids[done]!;
    reviewGame(game);
    const s = games.get(game)!.game.summary;
    emit("review.upserted", "games", { game, accuracy: s.accuracy! });
    emit("game.upserted", "games", { ...s });
    done++;
    w.progress = { done, total: ids.length, unit: "games", eta_seconds: 4 * (ids.length - done), text: null };
    if (done >= ids.length) {
      w.status = "done";
      w.ended_at = new Date().toISOString();
    }
    emit("work.upserted", "work", { ...w });
    if (done < ids.length) setTimeout(step, 4000);
  };
  setTimeout(step, 4000);
}

function act(g: Game, action: SeatAction, body: { uci?: string }): Response | undefined {
  if (!g.actions.includes(action)) return error(409, "conflict", "You cannot do that now.");
  const yourSide = g.summary.you!;
  const id = g.summary.id;
  if (action === "move") {
    const pos = fenOf(g);
    if (pos.turn !== yourSide) return error(409, "conflict", "It is not your turn.");
    if (!body.uci || !playMove(g, body.uci, 4000, null)) return error(400, "bad_request", "That move is not legal.");
    if (g.summary.status === "live") {
      const engine = g.summary.white.player === "dameless" || g.summary.black.player === "dameless";
      think(g, pos.turn === "white" ? "black" : "white", engine ? 1600 : 2600, () => {
        const pick = choices(g);
        const r = rng(g.moves.length);
        const uci = pick[Math.floor(r() * Math.min(pick.length, 6))];
        if (uci) playMove(g, uci, 1600, engine ? { cp: -30 + Math.round(r() * 60) } : null);
      });
    }
  } else if (action === "resign") {
    finish(g, yourSide === "white" ? "black_wins" : "white_wins", "resignation");
  } else if (action === "offer_draw") {
    emit("game.changed", `game:${id}`, {
      id,
      offers: { draw: yourSide, takeback: null },
      actions: g.actions,
      clocks: g.clocks,
      opponent_gone: null,
    });
    setTimeout(() => {
      emit("game.changed", `game:${id}`, {
        id,
        offers: { draw: null, takeback: null },
        actions: g.actions,
        clocks: g.clocks,
        opponent_gone: null,
      });
    }, 1500);
  } else if (action === "takeback") {
    const keep = Math.max(0, g.moves.length - (fenOf(g).turn === yourSide ? 2 : 1));
    g.moves.splice(keep);
    g.summary.plies = keep;
    emit("game.undone", `game:${id}`, { id, plies: keep });
  }
  return undefined;
}

let nextGameId = 500;
function newGame(body: {
  time?: TimeControl;
  white?: string;
  color?: Side | "random";
  version?: number;
  from?: { fen: string; moves: string[] };
}) {
  const tc = body.time ?? clockTc(5, 3);
  const side: Side =
    body.color === "random" ? (Math.random() < 0.5 ? "white" : "black") : body.white === "dameless" ? "black" : "white";
  const version = body.version ?? 14;
  let startFen: string | null = null;
  if (body.from) {
    const pos = setup(body.from.fen);
    if (pos) {
      for (const m of body.from.moves) playUci(pos, m);
      startFen = makeFen(pos.toSetup());
    }
  }
  const id = nextGameId++;
  const s = summary({
    id,
    white: side === "white" ? you() : dameless(version),
    black: side === "white" ? dameless(version) : you(),
    source: { type: "local" },
    time: tc,
    status: "live",
    started_at: new Date().toISOString(),
    you: side,
  });
  const ms = tc.type === "clock" ? tc.initial_ms : 0;
  add(s, [], {
    start_fen: startFen,
    clocks: tc.type === "clock" ? { white: ms, black: ms } : null,
    actions: ["move", "offer_draw", "resign", "abort"],
  });
  emit("game.upserted", "games", { ...s });
  const g = games.get(id)!.game;
  if (fenOf(g).turn !== side) {
    think(g, other(side), 1600, () => {
      const uci = choices(g)[0];
      if (uci) playMove(g, uci, 1600, { cp: 20 });
    });
  }
  return s;
}

// ---- Routing

function json(data: unknown, status = 200): Response {
  return new Response(JSON.stringify(data), { status, headers: { "Content-Type": "application/json" } });
}
function error(status: number, code: string, message: string): Response {
  return json({ error: { code, message } }, status);
}

const clone = <T>(v: T): T => structuredClone(v);

/** The stand-in for `fetch`: answers the API's routes from the data above, after a short pause like a real server. */
export async function fixtureFetch(input: string, init?: RequestInit): Promise<Response> {
  const url = new URL(input, location.origin);
  const method = (init?.method ?? "GET").toUpperCase();
  const path = url.pathname;
  const body = init?.body ? (JSON.parse(String(init.body)) as Record<string, unknown>) : {};
  startLive();
  if (path === "/api/events") return streamResponse(url, init?.signal);
  await new Promise((r) => setTimeout(r, 40 + Math.random() * 80));

  let m: RegExpMatchArray | null;
  if (method === "GET") {
    if (path === "/api/health") return json({ ok: true });
    if (path === "/api/status") return json(status);
    if (path === "/api/games") return json([...games.values()].map((s) => clone(s.game.summary)));
    if ((m = path.match(/^\/api\/games\/(\d+)$/))) {
      const s = games.get(Number(m[1]));
      return s ? json(clone(s.game)) : error(404, "not_found", "No such game.");
    }
    if ((m = path.match(/^\/api\/games\/(\d+)\/review$/))) {
      const s = games.get(Number(m[1]));
      return s?.review ? json(clone(s.review)) : error(404, "not_found", "This game has not been reviewed.");
    }
    if (path === "/api/ideas") return json(clone(ideas));
    if ((m = path.match(/^\/api\/ideas\/(\d+)$/))) {
      const idea = ideas.find((i) => i.id === Number(m![1]));
      return idea ? json(clone(idea)) : error(404, "not_found", "No such idea.");
    }
    if ((m = path.match(/^\/api\/ideas\/(\d+)\/trials\/(\d+)$/))) {
      const t = trials.find((x) => x.id === Number(m![2]));
      return t ? json(clone(t)) : error(404, "not_found", "No such trial.");
    }
    if (path === "/api/work") return json(clone(work));
    if (path === "/api/connections") return json(clone(connections));
    if (path === "/api/versions") return json(clone(versions));
  }
  if (method === "POST") {
    if (path === "/api/games") return json(newGame(body as { time?: TimeControl; color?: Side }), 201);
    if ((m = path.match(/^\/api\/games\/(\d+)\/actions\/(\w+)$/))) {
      const s = games.get(Number(m[1]));
      if (!s) return error(404, "not_found", "No such game.");
      return act(s.game, m[2] as SeatAction, body as { uci?: string }) ?? json(clone(s.game));
    }
    if (path === "/api/reviews") {
      if (status.policy.assist !== "allowed") return error(409, "fair_play", status.policy.reason ?? "");
      queueReview((body["games"] as GameId[]) ?? []);
      return json({}, 202);
    }
    if (path === "/api/eval") {
      if (status.policy.assist !== "allowed") return error(409, "fair_play", status.policy.reason ?? "");
      return json({ session: startEval(body as { fen: string; moves: string[] }) });
    }
    if ((m = path.match(/^\/api\/work\/(\d+)\/(stop|pause|resume)$/))) {
      const w = work.find((x) => x.id === Number(m![1]));
      if (!w) return error(404, "not_found", "No such work.");
      w.status = m[2] === "stop" ? "stopped" : m[2] === "pause" ? "paused" : "running";
      if (m[2] === "stop") w.ended_at = new Date().toISOString();
      w.paused_for = null;
      emit("work.upserted", "work", { ...w });
      return json(clone(w));
    }
  }
  if (method === "DELETE" && (m = path.match(/^\/api\/eval\/(\w+)$/))) {
    clearInterval(sessions.get(m[1]!));
    sessions.delete(m[1]!);
    return new Response(null, { status: 204 });
  }
  return error(404, "not_found", `The fixtures have no ${method} ${path}.`);
}
