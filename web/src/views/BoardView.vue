<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { RouterLink, useRoute, useRouter } from "vue-router";
import { useQuery } from "@pinia/colada";
import type { DrawShape } from "@lichess-org/chessground/draw";
import type { Key } from "@lichess-org/chessground/types";
import {
  PhArrowsDownUp,
  PhCaretLeft,
  PhCaretRight,
  PhChartLine,
  PhClipboardText,
  PhClockCounterClockwise,
  PhFlask,
  PhPause,
  PhPlay,
  PhSkipBack,
  PhSkipForward,
  PhSquaresFour,
} from "@phosphor-icons/vue";
import { useShortcuts } from "@eos-ui/vue";
import { EBreadcrumb } from "@eos-ui/vue/components/EBreadcrumb";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ECopyButton } from "@eos-ui/vue/components/ECopyButton";
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
import MoveList from "../board/MoveList.vue";
import Playlist from "../board/Playlist.vue";
import Timeline from "../board/Timeline.vue";
import Term from "../components/Term.vue";
import Workspace from "../components/Workspace.vue";
import { readGame } from "../board/game";
import { outcome, type Outcome } from "../board/playlist";
import { play, readPosition, sanLine, sanMoves, toPgn } from "../board/position";
import { useAnalysis, type Seen } from "../board/useAnalysis";
import { api } from "../api/runner";
import { defaultBuild, unwrap, useBuilds, useExperiment, useExperiments, useGames, useTest } from "../api/queries";
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

function go(to: number) {
  cursor.value = Math.max(0, Math.min(moves.value.length, to));
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
if (!isGame.value) readQuery();
watch(isGame, (g) => {
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
const game = computed(() => (isGame.value && gameState.value.data ? readGame(gameState.value.data) : undefined));
const gameFailed = computed(() => isGame.value && gameState.value.status === "error");
const loading = computed(() => isGame.value && !game.value && gameState.value.status !== "error");

const devWhite = computed(() => game.value?.headers.White === "dev");
const freeOrientation = stored<"white" | "black">("board.orientation", "white");
const gameOrientation = ref<"white" | "black">("white");
const orientation = computed({
  get: () => (isGame.value ? gameOrientation.value : freeOrientation.value),
  set: (v) => (isGame.value ? (gameOrientation.value = v) : (freeOrientation.value = v)),
});
const flip = () => (orientation.value = orientation.value === "white" ? "black" : "white");

// A game comes in at its start (or at the move the address names, once), with the new version at the bottom.
let wantedPly: number | null = typeof route.query.ply === "string" ? Number(route.query.ply) : null;
watch(
  game,
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

// The address follows the board, so a position can be bookmarked or shared.
let addressing: ReturnType<typeof setTimeout> | undefined;
function writeAddress() {
  if (loading.value) return;
  const query: Record<string, string> = {};
  if (isGame.value) {
    if (cursor.value) query.ply = String(cursor.value);
  } else {
    if (start.value !== "startpos") query.fen = start.value;
    if (cursor.value) query.moves = moves.value.slice(0, cursor.value).join(",");
  }
  if (build.value && route.query.build) query.build = build.value;
  void router.replace({ query });
}
// Stepping or playing changes the move many times a second; the address only needs the place it settles on.
watch([start, moves, cursor, isGame], () => {
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
const engineOn = computed({
  get: () => (isGame.value ? gameEngine.value : freeEngine.value),
  set: (v) => (isGame.value ? (gameEngine.value = v) : (freeEngine.value = v)),
});
const multipv = stored("board.lines", "3");

const playing = ref(false);
// The engine rests while the moves play on their own.
const searching = computed(() => engineOn.value && !playing.value && !loading.value);
const { lines, error, evals } = useAnalysis({ start, position, build, multipv, active: searching });
const best = computed(() => lines.value[0]);

const shapes = computed<DrawShape[]>(() =>
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

// ---------------------------------------------------------------------------------------------------------------------
// Evaluations: the engine's, and for a game what the players recorded
// ---------------------------------------------------------------------------------------------------------------------

const evalText = (e: number) => (Math.abs(e) >= 100 ? (e > 0 ? "#" : "#-") : signed(e));

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
const shown = computed(() => (engineOn.value && live.value ? live.value : (recorded.value ?? live.value)));
const fromEngine = computed(() => !!shown.value && shown.value === live.value && (engineOn.value || !recorded.value));

/** Mover's name for the move on the board, in a game. */
const mover = computed(() => {
  if (!played.value) return "";
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
  if (isGame.value) {
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
const games = computed(() => gamesState.value.data ?? []);

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
watch(game, () => {
  if (playing.value) schedule();
});

// ---------------------------------------------------------------------------------------------------------------------
// Starting points: paste, recent, the latest test
// ---------------------------------------------------------------------------------------------------------------------

const importOpen = ref(false);
const importText = ref("");
const importError = ref("");

function load() {
  const line = readPosition(importText.value);
  if (!line) {
    importError.value = "That is not a FEN or a PGN with moves.";
    return;
  }
  importOpen.value = false;
  importText.value = "";
  importError.value = "";
  if (isGame.value) {
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
  if (isGame.value || (start.value === "startpos" && !moves.value.length)) return;
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
    : toPgn({ fen: start.value, moves: moves.value.slice(0, cursor.value) });

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
// Keyboard
// ---------------------------------------------------------------------------------------------------------------------

// Arrows repeat when held, which `useShortcuts` does not.
function onKey(e: KeyboardEvent) {
  if (e.defaultPrevented || e.ctrlKey || e.metaKey || e.altKey) return;
  if (e.target instanceof Element && e.target.closest("input, textarea, select, [contenteditable], [role=dialog]"))
    return;
  if (e.key === "ArrowLeft") go(cursor.value - 1);
  else if (e.key === "ArrowRight") go(cursor.value + 1);
  else return;
  e.preventDefault();
}
onMounted(() => window.addEventListener("keydown", onKey));
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
useShortcuts({
  home: () => go(0),
  end: () => go(Infinity),
  space: toggle,
  f: flip,
  e: () => (engineOn.value = !engineOn.value),
  n: () => openGame(nextGame.value?.index),
  p: () => openGame(previousGame.value?.index),
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
  <Workspace library-label="Games and positions" inspector-label="Engine and moves">
    <!-- Library: a game's playlist, or the places a free board can start from. -->
    <template #library-head>
      <EBreadcrumb
        v-if="isGame"
        :link-component="RouterLink"
        :items="[
          { label: 'Lab', to: '/lab' },
          ...(experiment ? [{ label: experiment.name, to: `/lab/${experiment.id}` }] : []),
          { label: 'Games' },
        ]"
      />
      <h2 v-else class="head-title">Positions</h2>
    </template>

    <template #library>
      <div v-if="isGame" class="games">
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
            {{ newest?.name }}
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
    </template>

    <!-- Stage: the eval bar beside the board, as large as the pane allows. -->
    <div class="stage" :class="{ loading }">
      <div class="rig">
        <EvalBar :pawns="shown?.pawns" :text="shown?.text" :orientation="orientation" />
        <ChessBoard
          :position="position"
          :orientation="orientation"
          movable
          :shapes="searching ? shapes : []"
          @move="playMoves([$event])"
        />
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
          :disabled="!moves.length && !(isGame && nextGame)"
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
        <span v-if="moves.length" class="of">/ {{ moves.length }}</span>
      </span>
      <ESegmented v-model="rate" label="Speed" hide-label size="sm" :options="speeds" class="rate" />
      <EIconButton :icon="PhArrowsDownUp" label="Flip board (F)" tooltip size="sm" @click="flip" />
    </template>

    <!-- Inspector: the engine, then the moves, as on Lichess. -->
    <template #inspector-head>
      <ESwitch v-model="engineOn" label="Engine" />
      <ESelect
        v-if="buildOptions.length"
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
        <header v-if="game" class="match">
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
        </header>

        <ENotice v-if="gameFailed" tone="danger" title="That game did not load" class="problem">
          It may have been removed, or the runner is not reachable.
        </ENotice>

        <section class="engine" aria-label="Engine">
          <EEmptyState v-if="noBuilds" title="No engine built yet" icon="info" size="sm" bare>
            Build Reckless once and it analyses here.
            <template #action>
              <EButton size="sm" @click="router.push('/engines')">Build an engine</EButton>
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

        <section class="moves" aria-label="Moves">
          <MoveList :sans="sans" :cursor="cursor" :start="start" @go="go" />
        </section>

        <footer class="tools">
          <ECopyButton :text="() => position.fen" label="Copy FEN" size="sm" />
          <ECopyButton :text="pgn" label="Copy PGN" size="sm" />
          <EButton v-if="isGame" variant="quiet" size="sm" :icon="PhChartLine" @click="fork([])">
            Analyse from here
          </EButton>
        </footer>
      </div>
    </template>
  </Workspace>

  <EDialog
    v-model:open="importOpen"
    title="Paste a game or position"
    description="A FEN, or a PGN from Lichess, Chess.com or anywhere."
  >
    <ETextarea
      v-model="importText"
      label="FEN or PGN"
      :rows="6"
      :error="importError || undefined"
      placeholder="1. e4 e5 2. Nf3 Nc6 …"
      data-autofocus
    />
    <template #actions>
      <EButton @click="load">Load</EButton>
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
}

.aside {
  margin-inline-start: auto;
  color: var(--e-muted);
}

.recent-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* Stage: the eval bar and the board are the largest square the pane's content box allows, centred on the axis with
   room to spare. The pane itself is the size container, so nothing is measured in script. */
.stage {
  --bar: var(--evalbar, 1.5rem);
  --gap: var(--e-gap-snug);
  --side: min(100cqh, 100cqw - var(--bar) - var(--gap));

  display: grid;
  place-content: center;
  min-height: 0;
  padding: var(--e-gap-snug);
  box-sizing: border-box;
  container-type: size;
}

.rig {
  display: grid;
  grid-template-columns: var(--bar) var(--side);
  gap: var(--gap);
  width: calc(var(--bar) + var(--gap) + var(--side));
  height: var(--side);
  transition: opacity var(--e-motion-fast, 0.15s);
}

.loading .rig {
  opacity: 0.5;
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

/* Inspector: sections divided by hairlines, each with the card padding. */
.pick {
  flex: 1;
  min-width: 8rem;
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
  min-width: 4.5rem;
  font: 600 var(--e-text-3xl) / 1 var(--e-font-mono);
  letter-spacing: -0.03em;
}

.dim {
  color: var(--e-muted);
}

.facts {
  display: grid;
  flex: 1;
  gap: var(--e-gap-tight);
  min-width: 0;
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

.moves {
  flex: 1 1 auto;
  min-height: 6rem;
  overflow: hidden;
}

.tools {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--e-gap-snug);
  padding: var(--e-gap-base) var(--ws-pad);
  border-block-end: 0;
}

@media (width < 64rem) {
  .stage {
    --side: min(100cqw - var(--bar) - var(--gap), 75dvh);

    container-type: inline-size;
  }

  .scrub {
    flex-basis: 100%;
    order: 2;
    margin-inline: 0;
  }

  .counter {
    margin-inline-start: auto;
  }

  .insp {
    height: auto;
  }

  .moves {
    flex: none;
    height: 18rem;
  }
}
</style>
