<script setup lang="ts">
import { computed, defineComponent, onBeforeUnmount, onMounted, ref, shallowRef, toRaw, watch, type Slots } from "vue";
import { RouterLink, useRoute, useRouter } from "vue-router";
import { useQuery } from "@pinia/colada";
import type { DrawShape } from "@lichess-org/chessground/draw";
import type { Key } from "@lichess-org/chessground/types";
import {
  PhArrowsDownUp,
  PhBroadcast,
  PhCaretLeft,
  PhCaretRight,
  PhChartLine,
  PhClipboardText,
  PhClockCounterClockwise,
  PhListBullets,
  PhFlask,
  PhPause,
  PhPlay,
  PhSword,
  PhSkipBack,
  PhSkipForward,
  PhSpeakerHigh,
  PhSpeakerSlash,
  PhSquaresFour,
} from "@phosphor-icons/vue";
import { useShortcuts } from "@eos-ui/vue";
import { EBreadcrumb } from "@eos-ui/vue/components/EBreadcrumb";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ECopyButton } from "@eos-ui/vue/components/ECopyButton";
import { EDrawer } from "@eos-ui/vue/components/EDrawer";
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESelect } from "@eos-ui/vue/components/ESelect";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import { ETextarea } from "@eos-ui/vue/components/ETextarea";
import ChessBoard from "../board/ChessBoard.vue";
import EngineLines from "../board/EngineLines.vue";
import EvalBar from "../board/EvalBar.vue";
import MatchEngines, { type EngineRow } from "../board/MatchEngines.vue";
import MatchList from "../board/MatchList.vue";
import MatchStatus from "../board/MatchStatus.vue";
import MoveList from "../board/MoveList.vue";
import Playlist from "../board/Playlist.vue";
import PlayerStrip from "../board/PlayerStrip.vue";
import Timeline from "../board/Timeline.vue";
import MatchDialog from "../components/MatchDialog.vue";
import Term from "../components/Term.vue";
import Workspace from "../components/Workspace.vue";
import { readGame, type Game } from "../board/game";
import { clockAt, sideScore, type Side } from "../board/matchText";
import { useMatch } from "../board/useMatch";
import { outcome, type Outcome } from "../board/playlist";
import { inspectPosition, play, sanLine, sanMoves, toPgn, whitePawns, whiteScore } from "../board/position";
import { playMoveSound, soundOf } from "../board/sound";
import { useAnalysis, type Seen } from "../board/useAnalysis";
import { api } from "../api/runner";
import {
  defaultBuild,
  unwrap,
  useBuilds,
  useExperiment,
  useExperiments,
  useGames,
  useMatches,
  useTest,
} from "../api/queries";
import { ending, signed, speed, winChance } from "../format";
import { stored } from "../stored";

const route = useRoute();
const router = useRouter();

// ---------------------------------------------------------------------------------------------------------------------
// What is on the board
// ---------------------------------------------------------------------------------------------------------------------

const isGame = computed(() => route.name === "game");
const testId = computed(() => (isGame.value ? Number(route.params.test) : 0));
const index = computed(() => (isGame.value ? Number(route.params.index) : -1));
/** A match between two builds, live or finished: the board replays it like a game. */
const isMatch = computed(() => route.name === "match");
const matchId = computed(() => (isMatch.value ? Number(route.params.id) : 0));
/** Whether the line on the board is a recorded one (a game or a match) rather than free analysis. */
const inGame = computed(() => isGame.value || isMatch.value);

// The line on the board: where it starts, every move entered (including ones stepped back past), and how many are
// played. Stepping back keeps the moves after; a different move replaces them. A game fills the same three.
const start = ref("startpos");
const moves = ref<string[]>([]);
const cursor = ref(0);

const position = computed(
  () => play({ fen: start.value, moves: moves.value.slice(0, cursor.value) }) ?? play({ fen: "startpos", moves: [] })!,
);
const sans = computed(() => sanMoves({ fen: start.value, moves: moves.value }));
// Moves that are not legal from the start are dropped once, so the list and the board agree.
watch(sans, (s) => {
  if (s.length < moves.value.length) {
    moves.value = moves.value.slice(0, s.length);
    cursor.value = Math.min(cursor.value, s.length);
  }
});

/** The library's drawer, for when it is not beside the board. Declared early: `reset` closes it during setup. */
const drawerOpen = ref(false);

/**
 * Whether the board follows a live match. Stepping back detaches it, like a live video, and new moves keep arriving
 * without moving the board; stepping to the end (or "Live") attaches it again.
 */
const attached = ref(true);

function go(to: number) {
  cursor.value = Math.max(0, Math.min(moves.value.length, to));
  if (isMatch.value) attached.value = cursor.value >= moves.value.length;
}
function reset(fen = "startpos", line: string[] = []) {
  drawerOpen.value = false;
  start.value = fen;
  moves.value = line;
  cursor.value = line.length;
}

/** The free board, read from the address. */
function readQuery() {
  const fen = typeof route.query.fen === "string" ? route.query.fen : "startpos";
  const line = typeof route.query.moves === "string" && route.query.moves ? route.query.moves.split(",") : [];
  reset(fen, line);
}
if (!inGame.value) readQuery();
watch(inGame, (g) => {
  playing.value = false;
  if (!g) readQuery();
});

// A game from the runner.
const gameQuery = useQuery({
  key: () => ["game", testId.value, index.value],
  enabled: () => isGame.value,
  query: () =>
    unwrap(
      api.GET("/api/tests/{id}/games/{index}", {
        params: { path: { id: testId.value, index: index.value } },
        parseAs: "text",
      }),
    ),
});
const gameState = gameQuery.state;
const fresh = computed(() => (isGame.value && gameState.value.data ? readGame(gameState.value.data) : undefined));
// Moving to another game keeps the last one on show until the new one has loaded, so nothing blanks or jumps: the
// board keeps its position, the header its height, the timeline its graph.
const held = shallowRef<Game>();
watch(
  fresh,
  (g) => {
    if (g) held.value = g;
  },
  { immediate: true },
);
watch(isGame, (g) => {
  if (!g) held.value = undefined;
});

// A match from the runner, kept current by its events. Moving to another match keeps the last on show until the new one
// arrives, as games do.
const { match: matchRaw, failed: matchFailed, clock, thinking } = useMatch(matchId);
const matchData = computed(() => (matchRaw.value?.id === matchId.value ? matchRaw.value : undefined));
const matchLive = computed(() => matchData.value?.status === "running");
/** The match as a game, so the move list, the timeline and the evaluation bar read it like any other. */
const matchGame = computed<Game | undefined>(() => {
  const m = matchData.value;
  if (!m) return undefined;
  return {
    headers: { White: m.white.name, Black: m.black.name, Result: m.result, Termination: m.reason ?? "" },
    startFen: m.start_fen,
    plies: m.moves.map((mv) => ({
      san: mv.san,
      uci: mv.uci,
      fen: mv.fen_after,
      eval: mv.eval ? whitePawns(mv.eval, mv.side) : null,
      depth: mv.depth ?? null,
      nodes: mv.nodes ?? null,
      time: mv.time_ms / 1000,
    })),
  };
});

const game = computed(() => (isGame.value ? (fresh.value ?? held.value) : isMatch.value ? matchGame.value : undefined));
const gameFailed = computed(
  () => (isGame.value && gameState.value.status === "error") || (isMatch.value && matchFailed.value),
);
const loading = computed(
  () =>
    (isGame.value && !fresh.value && gameState.value.status !== "error") ||
    (isMatch.value && !matchData.value && !matchFailed.value),
);

const devWhite = computed(() => game.value?.headers.White === "dev");
// Flipping is for this visit only: the board opens with White at the bottom, so a stale flip never surprises.
const freeOrientation = ref<"white" | "black">("white");
const gameOrientation = ref<"white" | "black">("white");
const orientation = computed({
  get: () => (inGame.value ? gameOrientation.value : freeOrientation.value),
  set: (v) => (inGame.value ? (gameOrientation.value = v) : (freeOrientation.value = v)),
});
const flip = () => (orientation.value = orientation.value === "white" ? "black" : "white");

// A game comes in at its start (or at the move the address names, once), with the new version at the bottom.
let wantedPly: number | null = typeof route.query.ply === "string" ? Number(route.query.ply) : null;
watch(
  fresh,
  (g) => {
    if (!g) return;
    start.value = g.startFen;
    moves.value = g.plies.map((p) => p.uci);
    cursor.value = Math.max(0, Math.min(moves.value.length, wantedPly ?? 0));
    wantedPly = null;
    gameOrientation.value = g.headers.White === "dev" ? "white" : "black";
  },
  { immediate: true },
);

// A match comes in at its end when it is live (and follows it), else at its start or the move the address names. Moves
// that arrive later are added to the line; the board follows them only while attached.
let syncedMatch = 0;
watch(
  matchData,
  (m) => {
    if (!m) return;
    if (syncedMatch !== m.id) {
      syncedMatch = m.id;
      start.value = m.start_fen;
      moves.value = m.moves.map((mv) => mv.uci);
      attached.value = m.status === "running";
      cursor.value =
        m.status === "running" ? moves.value.length : Math.max(0, Math.min(moves.value.length, wantedPly ?? 0));
      wantedPly = null;
      gameOrientation.value = "white";
    } else if (m.moves.length > moves.value.length) {
      moves.value = [...moves.value, ...m.moves.slice(moves.value.length).map((mv) => mv.uci)];
      if (attached.value) cursor.value = moves.value.length;
    }
  },
  { immediate: true },
);

// The address follows the board, so a position can be bookmarked or shared.
let addressing: ReturnType<typeof setTimeout> | undefined;
function writeAddress() {
  if (loading.value) return;
  const query: Record<string, string> = {};
  if (inGame.value) {
    if (cursor.value && !(matchLive.value && attached.value)) query.ply = String(cursor.value);
  } else {
    if (start.value !== "startpos") query.fen = start.value;
    if (cursor.value) query.moves = moves.value.slice(0, cursor.value).join(",");
  }
  if (build.value && route.query.build) query.build = build.value;
  void router.replace({ query });
}
// Stepping or playing changes the move many times a second; the address only needs the place it settles on.
watch([start, moves, cursor, inGame], () => {
  clearTimeout(addressing);
  addressing = setTimeout(writeAddress, 300);
});
onBeforeUnmount(() => clearTimeout(addressing));

// ---------------------------------------------------------------------------------------------------------------------
// The engine
// ---------------------------------------------------------------------------------------------------------------------

const { state: builds } = useBuilds();
const build = ref(typeof route.query.build === "string" ? route.query.build : "");
watch(
  () => builds.value.data,
  (list) => {
    if (!list?.length) return;
    if (!list.some((b) => String(b.id) === build.value)) build.value = String(defaultBuild(list)!.id);
  },
  { immediate: true },
);
// Only a build the person chose goes in the address; the default is not worth a parameter.
watch(build, (b, before) => {
  if (before && b !== before && builds.value.data) void router.replace({ query: { ...route.query, build: b } });
});
const buildOptions = computed(() =>
  (builds.value.data ?? []).map((b) => ({
    value: String(b.id),
    label: `${b.ref} · ${b.commit.slice(0, 7)}`,
    description: b.subject,
  })),
);
const noBuilds = computed(() => builds.value.status === "success" && builds.value.data.length === 0);

// Free analysis keeps the engine on; a game opens with it off, so stepping through costs nothing until it is wanted.
const freeEngine = stored("board.engine", true);
const gameEngine = stored("board.engine.game", false);
// The analysis engine rests while a match is live: the two engines playing it have the machine.
const engineOn = computed({
  get: () => !matchLive.value && (inGame.value ? gameEngine.value : freeEngine.value),
  set: (v) => {
    if (matchLive.value) return;
    if (inGame.value) gameEngine.value = v;
    else freeEngine.value = v;
  },
});
const multipv = stored("board.lines", "3");

const playing = ref(false);
// The engine rests while the moves play on their own.
const searching = computed(() => engineOn.value && !playing.value && !loading.value);
const { lines, error, evals } = useAnalysis({ start, position, build, multipv, active: searching });
const best = computed(() => lines.value[0]);

const analysisArrows = computed<DrawShape[]>(() =>
  lines.value
    .filter(Boolean)
    .map((l, i) => ({
      orig: l.pv[0]!.slice(0, 2) as Key,
      dest: l.pv[0]!.slice(2, 4) as Key,
      brush: i === 0 ? "green" : "paleGreen",
      modifiers: { lineWidth: i === 0 ? 12 : 7 },
    }))
    .reverse(),
);

// A match draws each engine in its own colour (the inspector uses the same two). Live, the engine on the move shows its
// best move and the move it expects back, in the other engine's colour. Replaying, the move played next shows.
const matchArrowsOn = stored("board.match.arrows", true);
const matchArrows = computed<DrawShape[]>(() => {
  const m = matchData.value;
  if (!m) return [];
  const brush = (side: Side) => (side === "white" ? "engineWhite" : "engineBlack");
  const arrow = (uci: string | undefined, side: Side, width: number): DrawShape[] =>
    uci && uci.length >= 4
      ? [
          {
            orig: uci.slice(0, 2) as Key,
            dest: uci.slice(2, 4) as Key,
            brush: brush(side),
            modifiers: { lineWidth: width },
          },
        ]
      : [];
  if (matchLive.value && attached.value) {
    const t = thinking.value[m.turn];
    if (!t || t.ply !== moves.value.length + 1) return [];
    const other = m.turn === "white" ? "black" : "white";
    return [...arrow(t.pv[1], other, 7), ...arrow(t.pv[0], m.turn, 12)];
  }
  const next = m.moves[cursor.value];
  return next ? arrow(next.uci, next.side, 12) : [];
});
/** Whether the board shows the match's arrows rather than the analysis engine's. */
const matchShapes = computed(() => isMatch.value && (matchLive.value || !searching.value));
const wanted = computed<DrawShape[]>(() =>
  matchShapes.value ? (matchArrowsOn.value ? matchArrows.value : []) : analysisArrows.value,
);
const arrowsVisible = computed(() => matchShapes.value || searching.value);
// The arrows hold still unless the moves they point along change, and a change settles for a moment first, so an engine
// that flips between two candidates while it searches does not make the board flicker. The first arrows show at once.
const shapes = shallowRef<DrawShape[]>([]);
const shapeKey = (list: DrawShape[]) => list.map((d) => `${d.orig}${d.dest}${d.brush}`).join();
let settling: ReturnType<typeof setTimeout> | undefined;
let settlingOn = "";
watch(wanted, (next) => {
  const key = shapeKey(next);
  if (key === shapeKey(shapes.value)) {
    clearTimeout(settling);
    settlingOn = "";
  } else if (!next.length || !shapes.value.length) {
    clearTimeout(settling);
    settlingOn = "";
    shapes.value = next;
  } else if (key !== settlingOn) {
    clearTimeout(settling);
    settlingOn = key;
    settling = setTimeout(() => {
      settlingOn = "";
      shapes.value = wanted.value;
    }, 220);
  }
});
onBeforeUnmount(() => clearTimeout(settling));

// ---------------------------------------------------------------------------------------------------------------------
// Evaluations: the engine's, and for a game what the players recorded
// ---------------------------------------------------------------------------------------------------------------------

const evalText = (e: number) => (Math.abs(e) >= 100 ? (e > 0 ? "Mate" : "−Mate") : signed(e));

/** The move just played on the board, when it belongs to a game. */
const played = computed(() => (game.value && cursor.value > 0 ? game.value.plies[cursor.value - 1] : undefined));

/** The last recorded evaluation at or before the move on the board. */
const recorded = computed<Seen | undefined>(() => {
  const plies = game.value?.plies;
  if (!plies) return undefined;
  for (let i = Math.min(cursor.value, plies.length) - 1; i >= 0; i--) {
    const p = plies[i]!;
    if (p.eval !== null) return { pawns: p.eval, text: evalText(p.eval), depth: p.depth ?? 0 };
  }
  return undefined;
});
const live = computed(() => evals.get(position.value.fen));
/** What the engine on the move in a live match thinks of the position now, while the board is at the latest move. */
const matchEval = computed<Seen | undefined>(() => {
  const m = matchData.value;
  if (!m || !matchLive.value || !attached.value) return undefined;
  const t = thinking.value[m.turn];
  if (!t || t.ply !== moves.value.length + 1) return undefined;
  return { pawns: whitePawns(t.score, m.turn), text: whiteScore(t.score, m.turn), depth: t.depth };
});
const shown = computed(
  () => matchEval.value ?? (engineOn.value && live.value ? live.value : (recorded.value ?? live.value)),
);
const fromEngine = computed(() => !!shown.value && shown.value === live.value && (engineOn.value || !recorded.value));

/** Mover's name for the move on the board, in a game. */
const mover = computed(() => {
  if (!played.value || isMatch.value) return "";
  const white = (cursor.value - 1) % 2 === 0;
  return white === devWhite.value ? "New version" : "Current version";
});

/** The move that leads to each position: `Start`, `1. e4`, `1… e5`. Index 0 is the start. */
const plyLabels = computed(() => {
  const parts = start.value.split(" ");
  const offset = start.value !== "startpos" && parts[1] === "b" ? 1 : 0;
  const first = start.value === "startpos" ? 1 : Number(parts[5] ?? 1);
  return [
    "Start",
    ...sans.value.map((san, i) => {
      const ply = i + offset;
      return `${first + Math.floor(ply / 2)}${ply % 2 === 0 ? "." : "…"} ${san}`;
    }),
  ];
});

/** The position after each ply, for the timeline. */
const fens = computed(() => {
  const first = play({ fen: start.value, moves: [] })!.fen;
  if (game.value) return [first, ...game.value.plies.map((p) => p.fen)];
  const out = [first];
  for (let i = 1; i <= moves.value.length; i++)
    out.push(play({ fen: start.value, moves: moves.value.slice(0, i) })!.fen);
  return out;
});
/** Pawns as a share of a win, so one big swing does not flatten the rest: -1 for Black winning, 1 for White. */
const gauge = (pawns: number) => (winChance(pawns * 100) - 0.5) * 2;
/** What the timeline shows at each position: the recorded evaluation, else the engine's. */
const timeline = computed(() => {
  const plies = game.value?.plies;
  const values: (number | null)[] = [];
  const texts: (string | null)[] = [];
  fens.value.forEach((f, i) => {
    const r = i > 0 ? plies?.[i - 1]?.eval : null;
    const e = r === null || r === undefined ? evals.get(f) : undefined;
    values.push(r !== null && r !== undefined ? gauge(r) : e ? gauge(e.pawns) : null);
    texts.push(r !== null && r !== undefined ? evalText(r) : (e?.text ?? null));
  });
  return { values, texts };
});

// ---------------------------------------------------------------------------------------------------------------------
// Moves, on the board and from the engine
// ---------------------------------------------------------------------------------------------------------------------

/** A line played off the game becomes free analysis from the same position. */
function fork(extra: string[]) {
  const query: Record<string, string> = {};
  if (start.value !== "startpos") query.fen = start.value;
  const line = [...moves.value.slice(0, cursor.value), ...extra];
  if (line.length) query.moves = line.join(",");
  if (build.value) query.build = build.value;
  void router.push({ path: "/board", query });
}

function playMoves(uci: string[]) {
  if (inGame.value) {
    if (uci.length === 1 && moves.value[cursor.value] === uci[0]) go(cursor.value + 1);
    else fork(uci);
    return;
  }
  for (const m of uci) {
    if (moves.value[cursor.value] !== m) moves.value = [...moves.value.slice(0, cursor.value), m];
    cursor.value++;
  }
}

/** The move counter: `24. Nf3`, or `24… Nf3` for Black's. */
const counter = computed(() => plyLabels.value[cursor.value] ?? "Start");

// ---------------------------------------------------------------------------------------------------------------------
// The playlist
// ---------------------------------------------------------------------------------------------------------------------

const { state: testState } = useTest(testId);
const test = computed(() => testState.value.data?.test);
const { state: experimentState } = useExperiment(() => test.value?.experiment ?? 0);
const experiment = computed(() => experimentState.value.data?.experiment);
const { state: gamesState } = useGames(testId);
// Thousands of rows that are only read: skip the reactive proxies.
const games = computed(() => toRaw(gamesState.value.data) ?? []);

const filter = ref<"all" | Outcome>("all");
const listed = computed(() =>
  filter.value === "all" ? games.value : games.value.filter((g) => outcome(g) === filter.value),
);
const nextGame = computed(() => listed.value.find((g) => g.index > index.value));
const previousGame = computed(() => listed.value.findLast((g) => g.index < index.value));

function openGame(i: number | undefined, replace = false) {
  if (i === undefined) return;
  const to = `/games/${testId.value}/${i}`;
  void (replace ? router.replace(to) : router.push(to));
}

// ---------------------------------------------------------------------------------------------------------------------
// Matches: the players, the thinking engines, the list, and what to do next
// ---------------------------------------------------------------------------------------------------------------------

const { state: matchesState } = useMatches();
const matchList = computed(() => matchesState.value.data ?? []);
const matchAt = computed(() => matchList.value.findIndex((m) => m.id === matchId.value));
const nextMatch = computed(() => matchList.value[matchAt.value + 1]?.id);
const previousMatch = computed(() => (matchAt.value > 0 ? matchList.value[matchAt.value - 1]?.id : undefined));
function openMatch(id: number | undefined) {
  if (id !== undefined && id !== matchId.value) void router.push(`/matches/${id}`);
}

const matchOpen = ref(false);
// The command palette sends `match=1`; the dialog opens and the parameter goes.
watch(
  () => route.query.match,
  (p) => {
    if (!p) return;
    matchOpen.value = true;
    void router.replace({ query: { ...route.query, match: undefined } });
  },
  { immediate: true },
);

const topSide = computed<Side>(() => (orientation.value === "white" ? "black" : "white"));
const bottomSide = computed<Side>(() => orientation.value);

/** One player's strip: who, the clock (counting down while it is thinking and the board is live), and the score. */
function strip(side: Side) {
  const m = matchData.value!;
  const player = m[side];
  const hasClock = m.time.type === "clock";
  const liveView = matchLive.value && attached.value;
  const c = clock.value;
  let ms: number | null = null;
  let at = 0;
  let ticking = false;
  if (hasClock) {
    if (liveView && c) {
      ms = c[side];
      at = c.at;
      ticking = c.ticking && c.turn === side;
    } else ms = clockAt(m, cursor.value, side);
  }
  const lastOwn = m.moves.slice(0, cursor.value).findLast((mv) => mv.side === side);
  return {
    side,
    name: player.name,
    commit: player.commit,
    ms,
    at,
    ticking,
    idle: lastOwn ? `${(lastOwn.time_ms / 1000).toFixed(2)} s` : "",
    active: liveView && position.value.turn === side,
    lowMs: hasClock ? Math.min(10_000, m.time.type === "clock" ? m.time.seconds * 150 : 0) : 0,
    score: matchLive.value || m.result === "*" ? undefined : sideScore(m.result, side),
  };
}

/** Both engines: what the one on the move is thinking, and for the other what it last played. */
const engineRows = computed<EngineRow[]>(() => {
  const m = matchData.value;
  if (!m) return [];
  const liveView = matchLive.value && attached.value;
  return (["white", "black"] as const).map((side) => {
    const t = thinking.value[side];
    if (liveView && t && m.turn === side && t.ply === moves.value.length + 1) {
      return {
        side,
        name: m[side].name,
        eval: whiteScore(t.score, side),
        depth: `Depth ${t.depth}/${t.seldepth}`,
        speed: speed(t.nps),
        line: sanLine(t.fen, t.pv.slice(0, 12)),
        thinking: true,
      };
    }
    const own = m.moves.slice(0, cursor.value).findLast((mv) => mv.side === side);
    return {
      side,
      name: m[side].name,
      eval: own?.eval ? whiteScore(own.eval, side) : null,
      depth: own?.depth != null ? `Depth ${own.depth}` : "",
      speed: own ? `${(own.time_ms / 1000).toFixed(2)} s` : "",
      line: own ? `Played ${own.san}` : "Has not moved yet",
      thinking: false,
    };
  });
});

const matchBusy = ref(false);
const matchError = ref("");
async function stopMatch() {
  matchBusy.value = true;
  matchError.value = "";
  try {
    await unwrap(api.DELETE("/api/matches/{id}", { params: { path: { id: matchId.value } } }));
  } catch (e) {
    matchError.value = (e as Error).message;
  } finally {
    matchBusy.value = false;
  }
}
/** The same match again from the same position, with the colours kept or swapped. */
async function again(swap: boolean) {
  const m = matchData.value;
  if (!m) return;
  matchBusy.value = true;
  matchError.value = "";
  try {
    const next = await unwrap(
      api.POST("/api/matches", {
        body: {
          white: swap ? m.black.build : m.white.build,
          black: swap ? m.white.build : m.black.build,
          time: m.time,
          opening: m.start_fen === "startpos" ? { type: "start" } : { type: "position", fen: m.start_fen, moves: [] },
          hash: m.hash,
          threads: m.threads,
          adjudicate: m.adjudicate,
        },
      }),
    );
    void router.push(`/matches/${next.id}`);
  } catch (e) {
    matchError.value = (e as Error).message;
  } finally {
    matchBusy.value = false;
  }
}
watch(matchId, () => (matchError.value = ""));

const libraryName = computed(() => (isMatch.value ? "Matches" : isGame.value ? "Games" : "Positions"));

// ---------------------------------------------------------------------------------------------------------------------
// Playing the moves
// ---------------------------------------------------------------------------------------------------------------------

const rate = stored("board.speed", "2");
let timer: ReturnType<typeof setTimeout> | undefined;
const atEnd = computed(() => cursor.value >= moves.value.length);

function schedule() {
  clearTimeout(timer);
  if (!playing.value) return;
  const wait = 1000 / Number(rate.value);
  // A finished game rests a moment before the next one starts.
  timer = setTimeout(step, atEnd.value && isGame.value ? Math.max(wait, 1500) : wait);
}
function step() {
  if (!playing.value) return;
  if (loading.value) return schedule();
  if (!atEnd.value) go(cursor.value + 1);
  else if (isGame.value && nextGame.value) openGame(nextGame.value.index, true);
  else playing.value = false;
  schedule();
}
function toggle() {
  if (matchLive.value) return;
  if (playing.value) {
    playing.value = false;
    return;
  }
  // Playing from the end starts the game again.
  if (atEnd.value && !(isGame.value && nextGame.value)) go(0);
  playing.value = true;
}
watch(playing, schedule);
watch(rate, schedule);
onBeforeUnmount(() => clearTimeout(timer));
// A game that finished loading while playing carries on from its start.
watch(fresh, () => {
  if (playing.value) schedule();
});

// ---------------------------------------------------------------------------------------------------------------------
// Starting points: paste, recent, the latest test
// ---------------------------------------------------------------------------------------------------------------------

const importOpen = ref(false);
const importText = ref("");
// The text is read a moment after typing stops, so a long PGN is not parsed on every key.
const importRead = ref("");
let reading: ReturnType<typeof setTimeout> | undefined;
watch(importText, (t) => {
  clearTimeout(reading);
  if (!t.trim()) importRead.value = "";
  else reading = setTimeout(() => (importRead.value = t), 180);
});
onBeforeUnmount(() => clearTimeout(reading));
const imported = computed(() => inspectPosition(importRead.value));
/** What the text holds, when it holds a position, and that position for the preview. */
const importLine = computed(() => ("line" in imported.value ? imported.value : undefined));
const importError = computed(() => ("error" in imported.value ? imported.value.error : ""));
const importPreview = computed(() => {
  const l = importLine.value?.line;
  return l ? play(l) : undefined;
});
const importFacts = computed(() => {
  const l = importLine.value;
  const p = importPreview.value;
  if (!l || !p) return "";
  const who = p.turn === "white" ? "White" : "Black";
  const n = l.line.moves.length;
  const what = l.kind === "pgn" && n ? `${n} move${n === 1 ? "" : "s"}. ` : "";
  return `${what}${who} to move${p.check ? ", in check" : ""}.`;
});
// Reading is pending while the text and what was read differ.
const importPending = computed(() => importText.value.trim() !== importRead.value.trim());
const importReady = computed(() => !importPending.value && !!importLine.value);

function load() {
  const line = importLine.value?.line;
  if (!line || importPending.value) return;
  importOpen.value = false;
  importText.value = "";
  importRead.value = "";
  if (inGame.value) {
    const query: Record<string, string> = {};
    if (line.fen !== "startpos") query.fen = line.fen;
    if (line.moves.length) query.moves = line.moves.join(",");
    void router.push({ path: "/board", query });
  } else {
    reset(line.fen, line.moves);
    remember();
  }
}

// The command palette sends `paste=1`; the dialog opens and the parameter goes.
watch(
  () => route.query.paste,
  (p) => {
    if (!p) return;
    importOpen.value = true;
    void router.replace({ query: { ...route.query, paste: undefined } });
  },
  { immediate: true },
);

interface Recent {
  fen: string;
  moves: string[];
}
const recent = stored<Recent[]>("board.recent", []);
function remember() {
  if (inGame.value || (start.value === "startpos" && !moves.value.length)) return;
  const entry = { fen: start.value, moves: moves.value.slice(0, cursor.value) };
  const key = (r: Recent) => `${r.fen}|${r.moves.join(",")}`;
  recent.value = [entry, ...recent.value.filter((r) => key(r) !== key(entry))].slice(0, 10);
}
let remembering: ReturnType<typeof setTimeout> | undefined;
watch([start, moves, cursor], () => {
  clearTimeout(remembering);
  remembering = setTimeout(remember, 2000);
});
onBeforeUnmount(() => {
  clearTimeout(remembering);
  remember();
});
function recentLabel(r: Recent) {
  if (!r.moves.length) return r.fen.split(" ")[0]!;
  return sanLine(r.fen, r.moves.slice(0, 6)) + (r.moves.length > 6 ? " …" : "");
}

const { state: experiments } = useExperiments();
const newest = computed(() => experiments.value.data?.reduce((a, b) => (b.id > a.id ? b : a)));
const { state: newestDetail } = useExperiment(() => newest.value?.id ?? 0);
const latestTest = computed(() => newestDetail.value.data?.tests.findLast((t) => t.games > 0));

// ---------------------------------------------------------------------------------------------------------------------
// Copying
// ---------------------------------------------------------------------------------------------------------------------

const pgn = () =>
  isGame.value && gameState.value.data
    ? gameState.value.data
    : toPgn({ fen: start.value, moves: moves.value.slice(0, isMatch.value ? undefined : cursor.value) });

// ---------------------------------------------------------------------------------------------------------------------
// The result, from the new version's side
// ---------------------------------------------------------------------------------------------------------------------

const result = computed(() => {
  const r = game.value?.headers.Result;
  if (r === "1/2-1/2") return { label: "Drawn", tone: "neutral" } as const;
  if (!r || r === "*") return { label: "Unfinished", tone: "neutral" } as const;
  return (r === "1-0") === devWhite.value
    ? ({ label: "New version won", tone: "success" } as const)
    : ({ label: "New version lost", tone: "danger" } as const);
});

// ---------------------------------------------------------------------------------------------------------------------
// The library: beside the board when there is room, in a drawer when the board would otherwise shrink
// ---------------------------------------------------------------------------------------------------------------------

// The library's body is written once and shown either in the pane or in the drawer.
let libraryBody: Slots["default"];
const DefineLibrary = defineComponent({
  setup:
    (_, { slots }) =>
    () => {
      libraryBody = slots.default;
      return null;
    },
});
const UseLibrary = defineComponent({ setup: () => () => libraryBody?.() });

const frame = ref<{ $el: HTMLElement }>();
const stacked = ref(false);
/** Whether the library fits beside a board that fills the height; it drops out when it does not. */
const roomy = ref(true);
/** The person's own choice, once they have made one. */
const pinned = stored<boolean | null>("board.library", null);
const libraryInline = computed(() => stacked.value || (pinned.value ?? roomy.value));
watch(libraryInline, (v) => {
  if (v) drawerOpen.value = false;
});
watch(
  () => route.fullPath,
  () => (drawerOpen.value = false),
);

function toggleLibrary() {
  if (libraryInline.value) pinned.value = false;
  else if (roomy.value) pinned.value = true;
  else drawerOpen.value = true;
}

let observer: ResizeObserver | undefined;
onMounted(() => {
  const el = frame.value!.$el;
  const rem = () => parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
  const measure = () => {
    const r = rem();
    const w = el.clientWidth;
    stacked.value = w < 64 * r;
    // The side the height allows, and the width the whole row needs for it (see the sizes in the styles below).
    const pad = 0.75 * r;
    const side = el.clientHeight - 5.45 * r - 2 * pad - (isMatch.value ? 6 * r : 0);
    const needed = 16 * r + 22 * r + 2 * r + 0.375 * r + 2 * pad + side;
    // A margin on the way back keeps it from flickering at the edge.
    if (roomy.value && w < needed) roomy.value = false;
    else if (!roomy.value && w > needed + 2.5 * r) roomy.value = true;
  };
  observer = new ResizeObserver(measure);
  observer.observe(el);
  measure();
});
onBeforeUnmount(() => observer?.disconnect());

// ---------------------------------------------------------------------------------------------------------------------
// Sound: off until asked for, a knock for a move, a heavier one for a capture, a ping for a check
// ---------------------------------------------------------------------------------------------------------------------

const sound = stored("board.sound", false);
watch(cursor, (to, from) => {
  // A single step makes its sound; jumps and held keys stay quiet (and the player drops steps that crowd each other).
  if (!sound.value || Math.abs(to - from) !== 1) return;
  playMoveSound(to > from ? soundOf(sans.value[to - 1]) : "move");
});
function toggleSound() {
  sound.value = !sound.value;
  if (sound.value) playMoveSound("move");
}

// ---------------------------------------------------------------------------------------------------------------------
// Keyboard
// ---------------------------------------------------------------------------------------------------------------------

// Arrows repeat when held, which `useShortcuts` does not.
function onKey(e: KeyboardEvent) {
  if (e.defaultPrevented || e.ctrlKey || e.metaKey || e.altKey) return;
  if (e.target instanceof Element && e.target.closest("input, textarea, select, [contenteditable], [role=dialog]"))
    return;
  if (e.key === "ArrowLeft") go(cursor.value - 1);
  else if (e.key === "ArrowRight") go(cursor.value + 1);
  else if (e.key === "ArrowUp") go(0);
  else if (e.key === "ArrowDown") go(Infinity);
  else return;
  e.preventDefault();
}
onMounted(() => window.addEventListener("keydown", onKey));
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
useShortcuts({
  home: () => go(0),
  end: () => go(Infinity),
  l: () => go(Infinity),
  space: toggle,
  f: flip,
  e: () => (engineOn.value = !engineOn.value),
  m: () => (matchOpen.value = true),
  n: () => (isMatch.value ? openMatch(nextMatch.value) : openGame(nextGame.value?.index)),
  p: () => (isMatch.value ? openMatch(previousMatch.value) : openGame(previousGame.value?.index)),
});

const speeds = [
  { value: "0.5", label: "0.5×" },
  { value: "1", label: "1×" },
  { value: "2", label: "2×" },
  { value: "4", label: "4×" },
];
const filters = [
  { value: "all", label: "All" },
  { value: "W", label: "Won" },
  { value: "D", label: "Drawn" },
  { value: "L", label: "Lost" },
];
</script>

<template>
  <DefineLibrary>
    <div v-if="isMatch" class="games">
      <div class="games-bar">
        <EButton variant="outline" size="sm" :icon="PhSword" @click="matchOpen = true">Watch a match</EButton>
        <span class="count num">{{ matchList.length }}</span>
      </div>
      <ESkeleton v-if="matchesState.status === 'pending'" :lines="6" label="Loading matches" class="pad" />
      <EEmptyState v-else-if="!matchList.length" title="No matches yet" icon="info" size="sm" bare class="pad">
        Start one and it is kept here to replay.
      </EEmptyState>
      <div v-else class="match-rows"><MatchList :matches="matchList" :current="matchId" @open="openMatch" /></div>
    </div>

    <div v-else-if="isGame" class="games">
      <div class="games-bar">
        <ESegmented v-model="filter" label="Show games" hide-label size="sm" :options="filters" />
        <span class="count num">{{ listed.length.toLocaleString() }}</span>
      </div>
      <ESkeleton v-if="gamesState.status === 'pending'" :lines="6" label="Loading games" class="pad" />
      <EEmptyState v-else-if="!listed.length" title="No games here" icon="info" size="sm" bare class="pad">
        {{ filter === "all" ? "This test has no games yet." : "No game ended that way. Try another filter." }}
      </EEmptyState>
      <Playlist v-else :games="listed" :current="index" @open="openGame($event)" />
    </div>

    <div v-else class="starts">
      <EButton variant="quiet" size="sm" block :icon="PhSquaresFour" class="start" @click="reset()">
        Start position
      </EButton>
      <EButton variant="quiet" size="sm" block :icon="PhClipboardText" class="start" @click="importOpen = true">
        Paste a game or FEN
      </EButton>
      <EButton variant="quiet" size="sm" block :icon="PhSword" class="start" @click="matchOpen = true">
        Watch a match
      </EButton>
      <template v-if="matchList.length">
        <h3 class="section">Matches</h3>
        <MatchList :matches="matchList.slice(0, 5)" @open="openMatch" />
      </template>
      <template v-if="latestTest">
        <h3 class="section">Latest test</h3>
        <EButton
          variant="quiet"
          size="sm"
          block
          :icon="PhFlask"
          class="start"
          @click="router.push(`/games/${latestTest.id}/0`)"
        >
          <span class="grow">{{ newest?.name }}</span>
          <span class="aside num">{{ latestTest.games.toLocaleString() }}</span>
        </EButton>
      </template>
      <template v-if="recent.length">
        <h3 class="section">Recent</h3>
        <EButton
          v-for="(r, i) in recent"
          :key="i"
          variant="quiet"
          size="sm"
          block
          :icon="PhClockCounterClockwise"
          class="start recent"
          @click="reset(r.fen, r.moves)"
        >
          <span class="recent-text num">{{ recentLabel(r) }}</span>
        </EButton>
      </template>
    </div>
  </DefineLibrary>
  <Workspace
    ref="frame"
    fit
    :class="['board-ws', { 'board-ws--solo': !libraryInline, 'board-ws--match': isMatch }]"
    library-label="Games, matches and positions"
    inspector-label="Engine and moves"
  >
    <!-- Library: a game's playlist, or the places a free board can start from. -->
    <template v-if="libraryInline" #library-head>
      <EBreadcrumb
        v-if="isGame"
        :link-component="RouterLink"
        :items="[
          { label: 'Lab', to: '/lab' },
          ...(experiment ? [{ label: experiment.name, to: `/lab/${experiment.id}` }] : []),
          { label: 'Games' },
        ]"
      />
      <h2 v-else class="head-title">{{ libraryName }}</h2>
    </template>

    <template v-if="libraryInline" #library>
      <UseLibrary />
    </template>

    <!-- Stage: the eval bar beside the board, as large as the pane allows. -->
    <div class="stage" :class="{ loading }">
      <div :class="['rig', { 'rig--match': isMatch && matchData }]">
        <PlayerStrip v-if="isMatch && matchData" v-bind="strip(topSide)" class="strip-top" />
        <EvalBar :pawns="shown?.pawns" :text="shown?.text" :orientation="orientation" class="rig-eval" />
        <ChessBoard
          :position="position"
          :orientation="orientation"
          :movable="!matchLive"
          :shapes="arrowsVisible ? shapes : []"
          class="rig-board"
          @move="playMoves([$event])"
        />
        <PlayerStrip v-if="isMatch && matchData" v-bind="strip(bottomSide)" class="strip-bottom" />
      </div>
    </div>

    <!-- Transport: VLC's row, with the eval graph as the timeline. -->
    <template #stage-foot>
      <div class="buttons">
        <EIconButton
          :icon="PhSkipBack"
          label="First move (Home)"
          tooltip
          size="sm"
          :disabled="cursor === 0"
          @click="go(0)"
        />
        <EIconButton
          :icon="PhCaretLeft"
          label="Back (Left arrow)"
          tooltip
          size="sm"
          :disabled="cursor === 0"
          @click="go(cursor - 1)"
        />
        <EIconButton
          :icon="playing ? PhPause : PhPlay"
          :label="playing ? 'Pause (Space)' : 'Play (Space)'"
          tooltip
          variant="primary"
          size="sm"
          :disabled="matchLive || (!moves.length && !(isGame && nextGame))"
          @click="toggle"
        />
        <EIconButton
          :icon="PhCaretRight"
          label="Forward (Right arrow)"
          tooltip
          size="sm"
          :disabled="atEnd"
          @click="go(cursor + 1)"
        />
        <EIconButton
          :icon="PhSkipForward"
          label="Last move (End)"
          tooltip
          size="sm"
          :disabled="atEnd"
          @click="go(Infinity)"
        />
        <EButton
          v-if="matchLive"
          size="sm"
          :variant="attached ? 'secondary' : 'primary'"
          :icon="PhBroadcast"
          :aria-pressed="attached"
          class="live"
          @click="go(Infinity)"
        >
          Live
        </EButton>
      </div>
      <Timeline
        class="scrub"
        label="Position in the game, with the evaluation. Click or drag to move."
        :values="timeline.values"
        :texts="timeline.texts"
        :moves="plyLabels"
        :cursor="cursor"
        @select="go"
      />
      <span class="counter num" aria-live="off">
        {{ counter }}
        <span v-if="moves.length" class="of" title="Moves played so far, counting each side's move"
          >/ {{ moves.length }}</span
        >
      </span>
      <EIconButton
        v-if="!stacked"
        :icon="PhListBullets"
        :label="`${libraryName} list`"
        tooltip
        size="sm"
        :variant="libraryInline ? 'primary' : undefined"
        :aria-pressed="libraryInline"
        @click="toggleLibrary"
      />
      <ESegmented
        v-if="!isMatch && !(stacked && !moves.length)"
        v-model="rate"
        label="Speed"
        hide-label
        size="sm"
        :options="speeds"
        class="rate"
      />
      <EIconButton
        :icon="PhArrowsDownUp"
        :label="orientation === 'black' ? 'Flip board (F), Black at the bottom' : 'Flip board (F)'"
        tooltip
        size="sm"
        class="flip"
        :variant="orientation === 'black' ? 'primary' : undefined"
        @click="flip"
      />
    </template>

    <!-- Inspector: the engine, then the moves, as on Lichess. -->
    <template #inspector-head>
      <ESwitch v-if="isMatch" v-model="matchArrowsOn" label="Arrows" />
      <ESwitch v-if="!matchLive" v-model="engineOn" label="Engine" />
      <ESelect
        v-if="buildOptions.length && !matchLive"
        v-model="build"
        label="Engine version"
        hide-label
        size="sm"
        :options="buildOptions"
        class="pick"
      />
    </template>

    <template #inspector>
      <div class="insp">
        <MatchStatus
          v-if="isMatch && matchData"
          :match="matchData"
          :attached="attached"
          :busy="matchBusy"
          @stop="stopMatch"
          @rematch="again(false)"
          @swap="again(true)"
        />
        <header v-else-if="loading && !game" class="match" aria-hidden="true">
          <ESkeleton :height="20" width="70%" />
          <ESkeleton :height="20" width="55%" />
        </header>
        <header v-else-if="isGame && game" class="match">
          <p class="who">
            <span :class="['chip', devWhite ? 'chip--white' : 'chip--black']" aria-hidden="true"></span>
            New version
            <span class="vs">vs</span>
            <span :class="['chip', devWhite ? 'chip--black' : 'chip--white']" aria-hidden="true"></span>
            Current version
          </p>
          <p class="how">
            <EStamp size="sm" :tone="result.tone" :label="result.label" />
            {{ ending(game.headers.Termination) }} after {{ Math.ceil(game.plies.length / 2) }} moves
          </p>
          <div v-if="stacked" class="hop">
            <EIconButton
              :icon="PhCaretLeft"
              label="Previous game (P)"
              tooltip
              size="sm"
              :disabled="!previousGame"
              @click="openGame(previousGame?.index)"
            />
            <EIconButton
              :icon="PhCaretRight"
              label="Next game (N)"
              tooltip
              size="sm"
              :disabled="!nextGame"
              @click="openGame(nextGame?.index)"
            />
          </div>
        </header>

        <ENotice v-if="matchError" tone="danger" title="That did not work" class="problem">
          {{ matchError }}
        </ENotice>
        <ENotice
          v-if="gameFailed"
          tone="danger"
          :title="`That ${isMatch ? 'match' : 'game'} did not load`"
          class="problem"
        >
          It may have been removed, or the runner is not reachable.
        </ENotice>

        <section v-if="isMatch && matchData" class="engine engine--match" aria-label="Engines in the match">
          <MatchEngines :rows="engineRows" />
        </section>

        <section v-if="!isMatch || engineOn" class="engine" aria-label="Engine">
          <EEmptyState v-if="noBuilds" title="No engine built yet" icon="info" size="sm" bare>
            Build Reckless once and it analyses here.
            <template #action>
              <EButton size="sm" @click="router.push('/engines')">Build an engine</EButton>
            </template>
          </EEmptyState>

          <template v-else>
            <p v-if="!engineOn && !shown" class="off">Engine off</p>
            <div v-else class="hero">
              <div class="big num" :class="{ dim: !shown }">
                <template v-if="shown">{{ shown.text }}</template>
                <template v-else-if="searching">…</template>
                <template v-else>–</template>
              </div>
              <div class="facts num">
                <p v-if="!engineOn">{{ shown ? "From the game" : "Engine off" }}</p>
                <p v-else-if="playing">Paused while playing</p>
                <p v-else-if="best">
                  <Term of="depth" inline>Depth</Term> {{ best.depth }}/{{ best.seldepth }} · {{ speed(best.nps) }}
                </p>
                <p v-else-if="error" class="bad">{{ error }}</p>
                <p v-else>Thinking…</p>
                <p v-if="played && mover" class="played">
                  {{ mover }}: <template v-if="fromEngine && recorded">{{ recorded.text }}, </template>
                  <template v-if="played.depth !== null">depth {{ played.depth }}</template>
                  <template v-if="played.time !== null">, {{ played.time.toFixed(2) }}s</template>
                </p>
              </div>
            </div>
            <div v-if="engineOn" class="lines-bar">
              <span>Lines</span>
              <ESegmented
                v-model="multipv"
                label="Lines to show"
                hide-label
                size="sm"
                :options="['1', '2', '3', '5']"
              />
            </div>
            <ENotice v-if="error && best" tone="danger" title="The engine could not start" :role="false">
              {{ error }}
            </ENotice>
            <EngineLines
              v-if="engineOn && !playing"
              :position="position"
              :lines="lines"
              :count="Number(multipv)"
              @play="playMoves"
            />
          </template>
        </section>

        <section class="pane-moves" aria-label="Moves">
          <MoveList :sans="sans" :cursor="cursor" :start="start" :loading="loading" @go="go" />
        </section>

        <footer class="tools">
          <ECopyButton :text="() => position.fen" label="Copy FEN" size="sm" />
          <ECopyButton :text="pgn" label="Copy PGN" size="sm" />
          <EButton v-if="inGame" variant="quiet" size="sm" :icon="PhChartLine" @click="fork([])">
            Analyse from here
          </EButton>
          <EIconButton
            :icon="sound ? PhSpeakerHigh : PhSpeakerSlash"
            :label="sound ? 'Mute move sounds' : 'Play move sounds'"
            tooltip
            size="sm"
            class="sound"
            :variant="sound ? 'primary' : undefined"
            :aria-pressed="sound"
            @click="toggleSound"
          />
        </footer>
      </div>
    </template>
  </Workspace>

  <MatchDialog v-model:open="matchOpen" :fen="start" :moves="moves.slice(0, cursor)" />

  <EDrawer v-model:open="drawerOpen" side="start" size="sm" :title="libraryName">
    <div class="drawer-body"><UseLibrary v-if="!libraryInline" /></div>
  </EDrawer>

  <EDialog
    v-model:open="importOpen"
    title="Paste a game or position"
    description="A FEN, or a PGN from Lichess, Chess.com or anywhere."
  >
    <div class="paste">
      <ETextarea
        v-model="importText"
        label="FEN or PGN"
        :rows="4"
        :error="!importPending && importError ? importError : undefined"
        :hint="importLine?.note"
        placeholder="1. e4 e5 2. Nf3 Nc6 …"
        data-autofocus
        @keydown.ctrl.enter.prevent="load"
        @keydown.meta.enter.prevent="load"
      />
      <!-- The result of reading it, so the person sees what Load will do before pressing it. -->
      <div class="preview" :class="{ 'preview--on': importReady }" aria-live="polite">
        <div class="preview-board">
          <ChessBoard v-if="importPreview" still :position="importPreview" />
        </div>
        <p class="preview-facts">{{ importReady ? importFacts : "Paste or type to see the position." }}</p>
      </div>
    </div>
    <template #actions>
      <EButton :disabled="!importReady" @click="load">Load</EButton>
      <EButton variant="quiet" @click="importOpen = false">Cancel</EButton>
    </template>
  </EDialog>
</template>

<style scoped>
/* Library */
.head-title {
  margin: 0;
  font: var(--e-type-h6);
}

.games {
  display: grid;
  grid-template-rows: auto minmax(0, 1fr);
  height: 100%;
}

.games-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--e-gap-snug);
  padding: var(--e-gap-tight) var(--ws-pad);
  border-block-end: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 40%, transparent);
}

.count {
  color: var(--e-muted);
  font: var(--e-type-small);
}

.pad {
  padding: var(--e-pad-control-sm);
}

.match-rows {
  min-height: 0;
  overflow-y: auto;
  overscroll-behavior: contain;
}

.starts {
  display: grid;
  align-content: start;
  padding-block: var(--e-gap-tight);
}

.section {
  margin: var(--e-gap-snug) 0 0;
  padding: var(--e-gap-tight) var(--ws-pad);
  color: var(--e-muted);
  font: var(--e-type-small);
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.starts :deep(.e-btn) {
  justify-content: flex-start;
  border-radius: 0;
  text-align: start;
  text-decoration: none;
}

.aside {
  margin-inline-start: auto;
  color: var(--e-muted);
}

.grow {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.recent-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* Stage: the board drives the layout. Its side is the largest square the frame's height allows (less the transport and
   padding), or what the width allows once the library and the inspector keep their minimum widths. The frame
   (Workspace with fit) is the size container and the stage column hugs the rig, so only the inspector absorbs slack. */
.board-ws {
  --ws-library: 16rem;
  --lib: var(--ws-library);
  --ws-inspector: 22rem;
  --col: 2rem;
  --gap: var(--e-gap-snug);
  --pad: var(--ws-pad, var(--e-pad-control));
  --foot: 5.45rem;
  --strip: 2.5rem;
  --extra: 0px;
  --side: min(
    100cqh - var(--foot) - var(--extra) - 2 * var(--pad),
    100cqw - var(--lib) - var(--ws-inspector, 24rem) - var(--col) - var(--gap) - 2 * var(--pad)
  );
}

.board-ws--solo {
  --lib: 0px;
}

/* A match puts a player's strip above and below the board. */
.board-ws--match {
  --extra: calc(2 * (var(--strip) + var(--gap)));
}

.drawer-body {
  display: grid;
  grid-template-rows: minmax(0, 1fr);
  height: calc(100dvh - 9rem);
}

.board-ws :deep(.ws-foot) {
  padding-inline: var(--pad);
}

.stage {
  display: grid;
  place-content: center;
  padding: var(--pad);
  box-sizing: border-box;
}

.rig {
  display: grid;
  grid-template-columns: var(--col) var(--side);
  gap: var(--gap);
  width: calc(var(--col) + var(--gap) + var(--side));
  height: var(--side);
  transition: opacity 0.2s;
}

.rig--match {
  grid-template-rows: var(--strip) minmax(0, 1fr) var(--strip);
  height: calc(var(--side) + var(--extra));
}

.rig--match .strip-top {
  grid-area: 1 / 2;
}

.rig--match .rig-eval {
  grid-area: 2 / 1;
}

.rig--match .rig-board {
  grid-area: 2 / 2;
}

.rig--match .strip-bottom {
  grid-area: 3 / 2;
}

/* A game that loads at once never flashes: the dimming only starts if it takes a moment. */
.loading .rig {
  opacity: 0.55;
  transition-delay: 0.18s;
}

/* Transport */
.buttons {
  display: flex;
  align-items: center;
  gap: var(--e-gap-tight);
}

/* The timeline runs the full width above the controls, as a seek bar does in a player. */
.scrub {
  flex: 1 0 100%;
  order: -1;
}

.counter {
  min-width: 6rem;
  margin-inline: var(--e-gap-base) auto;
  font: 600 var(--e-text-sm) / 1 var(--e-font-mono);
  text-align: start;
  white-space: nowrap;
}

.of {
  color: var(--e-muted);
  font-weight: 400;
}

.rate {
  margin-inline-start: var(--e-gap-snug);
}

.off {
  margin: 0;
  padding-inline: var(--ws-pad);
  color: var(--e-muted);
  font: var(--e-type-small);
}

.live {
  margin-inline-start: var(--e-gap-snug);
}

/* Paste: the text, and under it what it was read as. */
.paste {
  display: grid;
  gap: var(--e-gap-base);
}

.preview {
  display: grid;
  grid-template-columns: 6.5rem minmax(0, 1fr);
  align-items: center;
  gap: var(--e-gap-base);
  min-height: 6.5rem;
}

.preview-board {
  width: 6.5rem;
  aspect-ratio: 1;
  border-radius: var(--e-radius-sm);
  background: color-mix(in oklab, var(--e-ink) 6%, var(--e-surface));
  box-shadow: inset 0 0 0 1px var(--e-border);
  opacity: 0.5;
  transition: opacity 0.2s;
}

.preview--on .preview-board {
  opacity: 1;
}

.preview-facts {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

/* Inspector: sections divided by hairlines, each with the card padding. */
.pick {
  flex: 1;
  min-width: 10rem;
}

.insp {
  --line: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 40%, transparent);

  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

.insp > * {
  flex: none;
  border-block-end: var(--line);
}

.match {
  display: grid;
  gap: var(--e-gap-snug);
  padding: var(--e-gap-base) var(--ws-pad);
}

.match:has(.hop) {
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: center;
}

.match:has(.hop) > p {
  grid-column: 1;
}

.hop {
  display: flex;
  grid-area: 1 / 2 / 3;
  gap: var(--e-gap-tight);
}

.match p {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--e-gap-snug);
  margin: 0;
}

.who {
  font-weight: 600;
}

.vs,
.how {
  color: var(--e-muted);
  font: var(--e-type-small);
  font-weight: 400;
}

.chip {
  width: 0.75rem;
  height: 0.75rem;
  border-radius: 50%;
  box-shadow: 0 0 0 1px var(--e-border-strong);
}

.chip--white {
  background: #fffaf3;
}

.chip--black {
  background: #2a2420;
}

.problem {
  margin: var(--e-gap-base) var(--e-pad-card);
}

.engine {
  display: grid;
  gap: var(--e-gap-snug);
  padding-block: var(--e-gap-base) var(--e-gap-snug);
}

.engine :deep(.e-empty) {
  padding-inline: var(--ws-pad);
}

.hero {
  display: flex;
  align-items: center;
  gap: var(--e-gap-loose);
  padding-inline: var(--ws-pad);
}

.big {
  min-width: 6.2ch;
  font: 600 var(--e-text-3xl) / 1 var(--e-font-mono);
  letter-spacing: -0.03em;
}

.dim {
  color: var(--e-muted);
}

.facts {
  display: grid;
  flex: 1;
  align-content: center;
  gap: var(--e-gap-tight);
  min-width: 0;
  min-height: 2.5rem;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.facts p {
  margin: 0;
}

.played {
  color: var(--e-ink);
}

.bad {
  color: var(--e-danger);
}

.lines-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding-inline: var(--ws-pad);
  color: var(--e-muted);
  font: var(--e-type-small);
}

.engine :deep(.lines) {
  margin-block-start: var(--e-gap-tight);
}

.pane-moves {
  flex: 1 1 auto;
  min-height: 6rem;
  overflow: hidden;
}

.tools :deep(.sound) {
  margin-inline-start: auto;
}

.tools {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--e-gap-snug);
  padding: var(--e-gap-base) var(--ws-pad);
  border-block-end: 0;
}

/* Below 64rem the panes stack: the board, the transport (a bar pinned to the bottom of the window, in thumb reach), then the inspector
   and the library. The board takes the width, and the transport is a small stack of rows: where we are, the timeline,
   then the buttons spread the width of the screen for thumbs. */
@media (width < 64rem) {
  .board-ws {
    --side: min(100cqw - var(--col) - var(--gap) - 2 * var(--pad), max(100dvh - 17rem - var(--extra), 16rem));
  }

  .board-ws :deep(.ws-stage) {
    display: contents;
  }

  .board-ws :deep(.ws-stage-body),
  .board-ws :deep(.ws-foot) {
    grid-row: auto;
  }

  .board-ws :deep(.ws-foot) {
    order: 9;
    position: sticky;
    inset-block-end: 0;
    z-index: 3;
    gap: var(--e-gap-tight) var(--e-gap-snug);
    padding-block: var(--e-gap-snug);
    background: var(--e-surface);
    box-shadow: 0 -1px 0 var(--e-border);
  }

  .counter {
    order: 1;
    flex: 1 1 auto;
    min-width: 0;
    margin-inline: var(--e-gap-tight) 0;
  }

  .rate {
    order: 2;
    margin-inline-start: 0;
  }

  .board-ws :deep(.flip) {
    order: 3;
  }

  .scrub {
    order: 4;
    flex: 1 0 100%;
    margin-inline: 0;
  }

  .buttons {
    order: 5;
    flex: 1 0 100%;
    justify-content: space-between;
  }

  .live {
    margin-inline-start: 0;
  }

  .insp {
    height: auto;
  }

  .pane-moves {
    flex: none;
    height: 18rem;
  }

  /* A long list scrolls inside its own box; the page keeps scrolling around it. */
  .games {
    height: auto;
  }

  .games :deep(.playlist) {
    height: 20rem;
  }

  .match-rows {
    max-height: 20rem;
  }
}

@media (width < 40rem) {
  .board-ws {
    --pad: 0.5rem;
    --col: 1.75rem;
  }
}
</style>
