<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, toRaw, watch } from "vue";
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
  PhCopy,
  PhFlask,
  PhMagnifyingGlass,
  PhPause,
  PhPlay,
  PhPlus,
  PhSword,
  PhSkipBack,
  PhSkipForward,
  PhSpeakerHigh,
  PhSpeakerSlash,
  PhSquaresFour,
  PhUser,
} from "@phosphor-icons/vue";
import { useCopy, useShortcuts, useToast } from "@eos-ui/vue";
import { EBreadcrumb } from "@eos-ui/vue/components/EBreadcrumb";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EMenu, type EMenuItem } from "@eos-ui/vue/components/EMenu";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESelect } from "@eos-ui/vue/components/ESelect";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import { ETabs } from "@eos-ui/vue/components/ETabs";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import ChessBoard from "../board/ChessBoard.vue";
import EngineLines from "../board/EngineLines.vue";
import EvalBar from "../board/EvalBar.vue";
import GameList from "../board/GameList.vue";
import GameSetup from "../board/GameSetup.vue";
import LoadDialog from "../board/LoadDialog.vue";
import MatchEngines, { type EngineRow } from "../board/MatchEngines.vue";
import MatchStatus from "../board/MatchStatus.vue";
import MoveList from "../board/MoveList.vue";
import Playlist from "../board/Playlist.vue";
import PlayerStrip from "../board/PlayerStrip.vue";
import PlayPanel from "../board/PlayPanel.vue";
import Timeline from "../board/Timeline.vue";
import Term from "../components/Term.vue";
import Workspace from "../components/Workspace.vue";
import { readGame, type Game } from "../board/game";
import { clockAt, sideScore, youSide, type Side } from "../board/matchText";
import { useMatch } from "../board/useMatch";
import { outcome, type Outcome } from "../board/playlist";
import { play, sanLine, sanMoves, toPgn, whitePawns, whiteScore } from "../board/position";
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

/**
 * What the free board is for: looking at a position with the engine (analyse), playing a build (play), or setting two
 * builds against each other (engines). The last two show their setup beside the board; a game in progress has its own
 * place at /matches/<id>.
 */
type Mode = "analyse" | "play" | "engines";
const mode = ref<Mode>("analyse");
const setup = computed(() => (!inGame.value && mode.value !== "analyse" ? mode.value : undefined));
const modes = [
  { value: "analyse", label: "Analyse", icon: PhMagnifyingGlass },
  { value: "play", label: "Play", icon: PhUser },
  { value: "engines", label: "Engines", icon: PhSword },
];
// The command palette and "Play from here" send `mode=` (or the older `match=1`); the mode is taken and the parameter goes.
watch(
  () => [route.query.mode, route.query.match],
  ([m, old]) => {
    const wanted = m === "play" || m === "engines" || m === "analyse" ? m : old ? "engines" : undefined;
    if (!wanted) return;
    mode.value = wanted;
    void router.replace({ query: { ...route.query, mode: undefined, match: undefined } });
  },
  { immediate: true },
);

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
const { match: matchRaw, failed: matchFailed, clock, thinking, takebacks } = useMatch(matchId);
const matchData = computed(() => (matchRaw.value?.id === matchId.value ? matchRaw.value : undefined));
const matchLive = computed(() => matchData.value?.status === "running");
/** The side you play, in a game against a build. */
const humanSide = computed(() => (matchData.value ? youSide(matchData.value) : undefined));
/** Your game is on, and you move on the board. */
const humanLive = computed(() => matchLive.value && !!humanSide.value);
/** Whether the engine's analysis of a game of yours has been asked for: until then, and while it is on, it stays hidden. */
const analysed = ref(false);
watch(matchId, () => (analysed.value = false));
const veiled = computed(() => !!humanSide.value && (matchLive.value || !analysed.value));
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
      gameOrientation.value = youSide(m) ?? "white";
    } else if (m.moves.length > moves.value.length) {
      moves.value = [...moves.value, ...m.moves.slice(moves.value.length).map((mv) => mv.uci)];
      if (attached.value) cursor.value = moves.value.length;
    }
  },
  { immediate: true },
);

// A takeback shortens the game: the line follows it back.
watch(takebacks, () => {
  const m = matchData.value;
  if (!m) return;
  moves.value = m.moves.map((mv) => mv.uci);
  cursor.value = moves.value.length;
  attached.value = true;
});

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
    if (!list.some((b) => String(b.id) === build.value && !b.deleted_at))
      build.value = String(defaultBuild(list)?.id ?? "");
  },
  { immediate: true },
);
// Only a build the person chose goes in the address; the default is not worth a parameter.
watch(build, (b, before) => {
  if (before && b !== before && builds.value.data) void router.replace({ query: { ...route.query, build: b } });
});
const buildOptions = computed(() =>
  (builds.value.data ?? [])
    .filter((b) => !b.deleted_at)
    .map((b) => ({
      value: String(b.id),
      label: `${b.ref} · ${b.commit.slice(0, 7)}`,
      description: b.subject,
    })),
);
const noBuilds = computed(() => builds.value.status === "success" && builds.value.data.every((b) => !!b.deleted_at));

// Free analysis keeps the engine on; a game opens with it off, so stepping through costs nothing until it is wanted.
const freeEngine = stored("board.engine", true);
const gameEngine = stored("board.engine.game", false);
// The analysis engine rests while a match is live: the two engines playing it have the machine.
const engineOn = computed({
  get: () =>
    !matchLive.value &&
    !setup.value &&
    (inGame.value ? (humanSide.value ? analysed.value : gameEngine.value) : freeEngine.value),
  set: (v) => {
    if (matchLive.value) return;
    if (humanSide.value) analysed.value = v;
    else if (inGame.value) gameEngine.value = v;
    else freeEngine.value = v;
  },
});
/** Whether the engine can be switched on and off, and its version picked: not while a match is live, a setup is open or your game keeps its verdict back (its own button turns the engine on). */
const engineChoice = computed(() => !matchLive.value && !setup.value && !veiled.value);
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
const arrowsVisible = computed(() => (matchShapes.value && !veiled.value) || searching.value);
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
/** Whether the eval bar has a place: not while your game hides it, and not when no engine or game has anything to show. */
const evalShown = computed(() => !veiled.value && !setup.value && (engineOn.value || !!shown.value));
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
    // Your game keeps the engine's opinion to itself until it is over.
    if (veiled.value) {
      values.push(null);
      texts.push(null);
      return;
    }
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

/** The address of the free board at a position. */
function freeBoard(fen: string, line: string[], extra: Record<string, string> = {}) {
  const query: Record<string, string> = { ...extra };
  if (fen !== "startpos") query.fen = fen;
  if (line.length) query.moves = line.join(",");
  void router.push({ path: "/board", query });
}

/** A line played off the game becomes free analysis from the same position. */
function fork(extra: string[], into?: Mode) {
  freeBoard(start.value, [...moves.value.slice(0, cursor.value), ...extra], {
    ...(into && { mode: into }),
    ...(build.value && { build: build.value }),
  });
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

/** Opens the setup for a game of your own or of two engines: on the free board it is a mode, elsewhere it goes there. */
function openSetup(into: Mode) {
  tab.value = "moves";
  if (inGame.value) void router.push({ path: "/board", query: { mode: into } });
  else mode.value = into;
}
/** Plays on from the board as it stands, whichever view it is. */
function playFromHere(into: Mode) {
  if (inGame.value) fork([], into);
  else mode.value = into;
}
function openPosition(fen: string, line: string[]) {
  mode.value = "analyse";
  tab.value = "moves";
  if (inGame.value) freeBoard(fen, line);
  else reset(fen, line);
}

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
    title: player.title,
    detail: [player.commit, player.rating].filter(Boolean).join(" · "),
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
  // A Lichess opponent or you report nothing to read: only our own engines have a row.
  const sides = (["white", "black"] as const).filter((side) => m[side].build != null);
  return sides.map((side) => {
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
  const seat = (p: { human?: boolean; build?: number | null }) => (p.human ? ("human" as const) : p.build);
  const w = m && seat(m.white);
  const b = m && seat(m.black);
  // A Lichess game has an opponent that is not one of our builds: there is nothing to replay it with.
  if (!m || w == null || b == null) return;
  matchBusy.value = true;
  matchError.value = "";
  try {
    const next = await unwrap(
      api.POST("/api/matches", {
        body: {
          white: swap ? b : w,
          black: swap ? w : b,
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

// ---------------------------------------------------------------------------------------------------------------------
// Playing against a build
// ---------------------------------------------------------------------------------------------------------------------

const board = ref<InstanceType<typeof ChessBoard>>();
const toast = useToast();

/** A move from the board: in your game it goes to the runner, which checks it and charges your clock. */
function onBoardMove(uci: string) {
  if (humanLive.value) void sendMove(uci);
  else playMoves([uci]);
}
async function sendMove(uci: string) {
  const id = matchId.value;
  const before = moves.value.length;
  // The piece is already where you put it: the line follows at once, and is put back if the runner says no.
  moves.value = [...moves.value, uci];
  cursor.value = moves.value.length;
  try {
    await unwrap(api.POST("/api/matches/{id}/moves", { params: { path: { id } }, body: { uci } }));
  } catch (e) {
    if (matchId.value === id && moves.value.length === before + 1 && moves.value[before] === uci) {
      moves.value = moves.value.slice(0, before);
      cursor.value = Math.min(cursor.value, before);
    }
    board.value?.reset();
    toast.show({ key: "move", title: "That move was not played", message: (e as Error).message, tone: "danger" });
  }
}
async function takeback() {
  matchError.value = "";
  try {
    await unwrap(api.POST("/api/matches/{id}/takeback", { params: { path: { id: matchId.value } } }));
  } catch (e) {
    matchError.value = (e as Error).message;
  }
}
async function offerDraw() {
  matchError.value = "";
  try {
    const { accepted } = await unwrap(api.POST("/api/matches/{id}/draw", { params: { path: { id: matchId.value } } }));
    if (!accepted) toast.show({ key: "draw", title: "The draw was declined", tone: "info" });
  } catch (e) {
    matchError.value = (e as Error).message;
  }
}
/** Turns the engine on for a finished game of yours, which shows its verdict. */
function analyseGame() {
  analysed.value = true;
  go(0);
}

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
/** A game or position from the load dialog opens on the board. */
function onLoad(fen: string, line: string[]) {
  openPosition(fen, line);
  remember();
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

/** The "New" menu: where a board can start from, and loading one. */
const newItems = computed(() => {
  const items: EMenuItem[] = [
    { label: "Start position", icon: PhSquaresFour, onClick: () => openPosition("startpos", []) },
    { label: "Load a game or FEN", icon: PhClipboardText, onClick: () => (importOpen.value = true) },
  ];
  if (latestItem.value) {
    const t = latestItem.value;
    items.push({ label: `Games of ${t.name}`, icon: PhFlask, onClick: () => void router.push(`/games/${t.id}/0`) });
  }
  if (recent.value.length) {
    items.push({ divider: true }, { type: "label", label: "Recent positions" });
    for (const r of recent.value.slice(0, 6)) {
      items.push({ label: recentLabel(r), icon: PhClockCounterClockwise, onClick: () => openPosition(r.fen, r.moves) });
    }
  }
  return items;
});

const { state: experiments } = useExperiments();
const newest = computed(() => experiments.value.data?.reduce((a, b) => (b.id > a.id ? b : a)));
const { state: newestDetail } = useExperiment(() => newest.value?.id ?? 0);
const latestTest = computed(() => newestDetail.value.data?.tests.findLast((t) => t.games > 0));
const latestItem = computed(() =>
  latestTest.value && newest.value
    ? { id: latestTest.value.id, name: newest.value.name, games: latestTest.value.games }
    : undefined,
);

// ---------------------------------------------------------------------------------------------------------------------
// Copying
// ---------------------------------------------------------------------------------------------------------------------

const pgn = () =>
  isGame.value && gameState.value.data
    ? gameState.value.data
    : toPgn({ fen: start.value, moves: moves.value.slice(0, isMatch.value ? undefined : cursor.value) });

const { copy } = useCopy();
async function copyAs(what: "FEN" | "PGN") {
  const ok = await copy(what === "FEN" ? position.value.fen : pgn());
  toast.show({
    key: "copy",
    title: ok ? `Copied the ${what === "FEN" ? "position" : "game"}` : "Couldn’t copy",
    tone: ok ? "success" : "danger",
  });
}
/** What to do with the position on the board of a game: look at it, play it, or set two engines on it. */
const continueItems = computed<EMenuItem[]>(() => [
  { label: "Analyse", icon: PhMagnifyingGlass, onClick: () => fork([]) },
  { label: "Play against an engine", icon: PhUser, onClick: () => playFromHere("play") },
  { label: "Watch two engines", icon: PhSword, onClick: () => playFromHere("engines") },
]);
const copyItems = [
  { label: "Position (FEN)", onClick: () => void copyAs("FEN") },
  { label: "Game (PGN)", onClick: () => void copyAs("PGN") },
];

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
// The right panel: Moves and Games. Below 75rem the panes stack, the board first.
// ---------------------------------------------------------------------------------------------------------------------

/** Which tab of the panel shows. */
const tab = ref("moves");
const liveGames = computed(() => matchList.value.filter((m) => m.status === "running").length);
const tabs = computed(() => [
  { id: "moves", label: "Moves" },
  { id: "games", label: "Games", count: liveGames.value || undefined },
]);

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

// The board's size step: eight device pixels, in CSS pixels. It follows the pixel ratio as the window moves between
// screens or the page is zoomed.
let stepQuery: MediaQueryList | undefined;
function setStep() {
  const ratio = window.devicePixelRatio || 1;
  document.documentElement.style.setProperty("--cg-step", `${8 / ratio}px`);
  stepQuery?.removeEventListener("change", setStep);
  stepQuery = matchMedia(`(resolution: ${ratio}dppx)`);
  stepQuery.addEventListener("change", setStep);
}
onMounted(setStep);
onBeforeUnmount(() => stepQuery?.removeEventListener("change", setStep));
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
useShortcuts({
  home: () => go(0),
  end: () => go(Infinity),
  l: () => go(Infinity),
  space: toggle,
  f: flip,
  e: () => (engineOn.value = !engineOn.value),
  m: () => openSetup("engines"),
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
  <Workspace
    fit
    :class="['board-ws', { 'board-ws--match': isMatch, 'board-ws--noeval': !evalShown }]"
    inspector-label="Moves and games"
  >
    <!-- Stage: the eval bar beside the board, as large as the pane allows. -->
    <div class="stage" :class="{ loading }">
      <div :class="['rig', { 'rig--match': isMatch && matchData }]">
        <PlayerStrip v-if="isMatch && matchData" v-bind="strip(topSide)" class="strip-top" />
        <!-- Your game keeps the engine's opinion to itself until it is over. -->
        <div v-if="!evalShown" class="rig-eval"></div>
        <EvalBar v-else :pawns="shown?.pawns" :text="shown?.text" :orientation="orientation" class="rig-eval" />
        <ChessBoard
          ref="board"
          :position="position"
          :orientation="orientation"
          :movable="humanLive ? attached : !matchLive"
          :premove="humanLive ? humanSide : undefined"
          :shapes="arrowsVisible ? shapes : []"
          class="rig-board"
          @move="onBoardMove"
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
      <ESegmented
        v-if="!isMatch && (isGame || moves.length)"
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

    <!-- Inspector: Moves (what to do with the board, the engine, the moves) and Games, as on Lichess. -->
    <template #inspector>
      <ETabs v-model="tab" :items="tabs" fill stretch label="Board panel" class="panel">
        <template #panel-moves>
          <div :class="['moves-head', { 'moves-head--game': inGame }]">
            <div class="head-row">
              <ESegmented
                v-if="!inGame"
                v-model="mode"
                label="What to do with the board"
                hide-label
                size="sm"
                :options="modes"
              />
              <EMenu label="New or load" :items="newItems" side="bottom" align="end" class="new">
                <template #trigger>
                  <EButton variant="outline" size="sm" :icon="PhPlus">New</EButton>
                </template>
              </EMenu>
            </div>
            <div v-if="(isMatch && !veiled) || engineChoice" class="head-row">
              <ESwitch v-if="isMatch && !veiled" v-model="matchArrowsOn" label="Arrows" />
              <ESwitch v-if="engineChoice" v-model="engineOn" label="Engine" />
              <ESelect
                v-if="buildOptions.length && engineChoice"
                v-model="build"
                label="Engine version"
                hide-label
                size="sm"
                :options="buildOptions"
                class="pick"
              />
            </div>
          </div>

          <div class="insp">
            <PlayPanel
              v-if="isMatch && matchData && humanSide"
              :match="matchData"
              :attached="attached"
              :busy="matchBusy"
              :analysing="analysed"
              @resign="stopMatch"
              @takeback="takeback"
              @draw="offerDraw"
              @analyse="analyseGame"
              @rematch="again(false)"
              @swap="again(true)"
              @fresh="openSetup('play')"
            />
            <MatchStatus
              v-else-if="isMatch && matchData"
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
              <div class="hop">
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

            <GameSetup v-if="setup" :mode="setup" :fen="start" :moves="moves.slice(0, cursor)" />

            <section
              v-if="isMatch && matchData && !veiled"
              class="engine engine--match"
              aria-label="Engines in the match"
            >
              <MatchEngines :rows="engineRows" />
            </section>

            <section
              v-if="!setup && !humanLive && (isMatch ? engineOn : engineOn || shown)"
              class="engine"
              aria-label="Engine"
            >
              <EEmptyState v-if="noBuilds" title="No engine built yet" icon="info" size="sm" bare>
                Build Reckless once and it analyses here.
                <template #action>
                  <EButton size="sm" @click="router.push('/versions')">Build a version</EButton>
                </template>
              </EEmptyState>

              <template v-else>
                <div class="hero">
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
              <MoveList :free="!inGame" :sans="sans" :cursor="cursor" :start="start" :loading="loading" @go="go" />
            </section>

            <footer class="tools">
              <EMenu label="Copy" :items="copyItems" side="top">
                <template #trigger>
                  <EButton variant="quiet" size="sm" :icon="PhCopy">Copy</EButton>
                </template>
              </EMenu>
              <EMenu
                v-if="inGame && !gameFailed && !humanLive && !(isMatch && matchLive)"
                label="Continue from here"
                :items="continueItems"
                side="top"
              >
                <template #trigger>
                  <EButton variant="quiet" size="sm" :icon="PhChartLine">Continue from here</EButton>
                </template>
              </EMenu>
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

        <template #panel-games>
          <div v-if="isGame" class="games">
            <EBreadcrumb
              :link-component="RouterLink"
              class="crumbs"
              :items="[
                { label: 'Lab', to: '/lab' },
                ...(experiment ? [{ label: experiment.name, to: `/lab/${experiment.id}` }] : []),
                { label: 'Games' },
              ]"
            />
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
          <GameList
            v-else
            :matches="matchList"
            :loading="matchesState.status === 'pending'"
            :current="matchId || undefined"
            @open="openMatch"
          />
        </template>
      </ETabs>
    </template>
  </Workspace>

  <LoadDialog v-model:open="importOpen" @load="onLoad" />
</template>

<style scoped>
/* The panel: tabs on top, each tab's content under them */
.panel {
  height: 100%;
  min-height: 0;
}

.moves-head {
  display: grid;
  gap: var(--e-gap-snug);
  padding: var(--e-gap-snug) var(--ws-pad);
  border-block-end: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 40%, transparent);
}

.head-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--e-gap-snug);
}

.head-row .new {
  margin-inline-start: auto;
}

/* A recorded game has no mode choice: its switches and New share one row, and the engine version takes the next. */
.moves-head--game {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
}

.moves-head--game .head-row {
  display: contents;
}

.moves-head--game .new {
  order: 1;
}

.moves-head--game .pick {
  flex-basis: 100%;
  order: 2;
}

.crumbs {
  padding: var(--e-gap-snug) var(--ws-pad);
}

/* The games tab */

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

/* Stage: the board drives the layout. Its side is the largest square the frame's height allows (less the transport and
   padding), or what the width allows once the panel keeps its width. The frame
   (Workspace with fit) is the size container and the stage column hugs the rig, so only the inspector absorbs slack. */
.board-ws {
  --ws-inspector: 22rem;
  --col: 2rem;
  --gap: var(--e-gap-snug);

  /* The gap between the eval bar and the board goes with the bar. */
  --egap: var(--gap);
  --pad: var(--ws-pad, var(--e-pad-control));
  --foot: 5.45rem;
  --strip: 2.5rem;
  --extra: 0px;
  --side-raw: min(
    100cqh - var(--foot) - var(--extra) - 2 * var(--pad),
    100cqw - var(--ws-inspector, 24rem) - var(--col) - var(--egap) - 2 * var(--pad)
  );
  --side: var(--side-raw);
}

/* chessground draws its squares on a whole number of device pixels, eight squares wide, and floors the rest: a board
   that is not a multiple of eight device pixels gets a sliver of empty frame on its right and bottom. The side is rounded
   down to that step (`--cg-step`, eight device pixels in CSS pixels, set from the screen's pixel ratio), so the squares
   meet the frame exactly. */
@supports (width: round(down, 10px, 1px)) {
  .board-ws {
    --side: round(down, var(--side-raw), var(--cg-step, 8px));
  }
}

/* On a wide screen the board is height-limited: the panel grows to a readable width and no further, and the board and
   panel sit together in the middle, as on Lichess, instead of the panel taking all the slack. The board column is its
   content's full width (max-content, not auto): with auto, the grid filled the panel to its maximum first and squeezed
   the board column under the board, so the panel overlapped it. */
@media (width >= 75rem) {
  .board-ws.ws--fit.ws--inspector {
    grid-template-columns: max-content minmax(var(--ws-inspector), 30rem);
    justify-content: center;
  }
}

/* A match puts a player's strip above and below the board. */
.board-ws--match {
  --extra: calc(2 * (var(--strip) + var(--gap)));
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
  gap: var(--gap) var(--egap);
  width: calc(var(--col) + var(--egap) + var(--side));
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

.live {
  margin-inline-start: var(--e-gap-snug);
}

/* Inspector: sections divided by hairlines, each with the card padding. */
.pick {
  flex: 1;
  min-width: 10rem;
}

.insp {
  --line: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 40%, transparent);

  display: flex;
  flex: 1 1 0;
  flex-direction: column;
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

/* The two engine cards are sections of their own: they meet the rows above and below at the hairline. */
.engine--match {
  padding-block: 0;
}

/* eos bleeds the tab strip and panel by their focus ring; here the pane's edge is the edge. */
.panel :deep(.e-tablist),
.panel :deep(.e-tabpanel) {
  margin-inline: 0;
  padding-inline: 0;
}

/* The Moves tab is a header bar and the inspector under it: the inspector takes what the bar leaves, so the tools row
   stays at the foot of the pane instead of below the fold. */
.panel :deep(.e-tabpanel:not([hidden])) {
  display: flex;
  flex-direction: column;
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

/* Below 75rem the panes stack: the board, the transport (a bar pinned to the bottom of the window, in thumb reach), then the inspector
   and the library. The board takes the width, and the transport is a small stack of rows: where we are, the timeline,
   then the buttons spread the width of the screen for thumbs. */
@media (width < 75rem) {
  .board-ws {
    /* The window less the header and the transport bar, which stay in view with the board. The status bar is below. */
    --chrome: 12rem;
    --side-raw: min(
      100cqw - var(--col) - var(--egap) - 2 * var(--pad),
      max(100dvh - var(--chrome) - var(--extra), 16rem)
    );
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

  .panel {
    height: auto;
  }

  .panel :deep(.e-tabpanel) {
    flex: none;
    overflow: visible;
  }

  .insp {
    flex: none;
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
}

/* A phone: where we are and the speed above the timeline, then the buttons spread the width of the screen for thumbs. */
@media (width < 40rem) {
  .board-ws {
    --pad: 0.5rem;
    --col: 1.75rem;
    --chrome: 17rem; /* the transport bar is three rows */
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
}

/* With no eval bar, the board and the strips and bar around it take its width. */
.board-ws--noeval {
  --col: 0px;
  --egap: 0px;
}
</style>
