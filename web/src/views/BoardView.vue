<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import type { DrawShape } from "@lichess-org/chessground/draw";
import type { Key } from "@lichess-org/chessground/types";
import {
  PhArrowCounterClockwise,
  PhArrowsDownUp,
  PhCaretDoubleLeft,
  PhCaretDoubleRight,
  PhCaretLeft,
  PhCaretRight,
  PhPlay,
  PhUploadSimple,
} from "@phosphor-icons/vue";
import { useShortcuts } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ECopyButton } from "@eos-ui/vue/components/ECopyButton";
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";
import { EInline } from "@eos-ui/vue/components/EInline";
import { EKbd } from "@eos-ui/vue/components/EKbd";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { EPageHeader } from "@eos-ui/vue/components/EPageHeader";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESelect } from "@eos-ui/vue/components/ESelect";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import { ETextarea } from "@eos-ui/vue/components/ETextarea";
import ChessBoard from "../board/ChessBoard.vue";
import { followCurrent } from "../board/follow";
import EvalBar from "../board/EvalBar.vue";
import LineChart from "../charts/LineChart.vue";
import Term from "../components/Term.vue";
import { play, readPosition, sanLine, sanMoves, toPgn, whitePawns, whiteScore } from "../board/position";
import { useRunnerEvent } from "../api/events";
import { api } from "../api/runner";
import { defaultBuild, unwrap, useBuilds, useStatus } from "../api/queries";
import type { components } from "../api/schema";
import { speed } from "../format";
import { stored } from "../stored";

type Info = components["schemas"]["Info"];

const route = useRoute();
const router = useRouter();

// The game on the board: where it starts, every move entered (including ones stepped back past), and how many are
// played. Stepping back keeps the moves after; a different move replaces them.
const start = ref(typeof route.query.fen === "string" ? route.query.fen : "startpos");
const moves = ref<string[]>(
  typeof route.query.moves === "string" && route.query.moves ? route.query.moves.split(",") : [],
);
const cursor = ref(moves.value.length);

const position = computed(
  () => play({ fen: start.value, moves: moves.value.slice(0, cursor.value) }) ?? play({ fen: "startpos", moves: [] })!,
);
const sans = computed(() => sanMoves({ fen: start.value, moves: moves.value }));
// Moves that are not legal from the start are dropped once, so the list and the board agree.
watch(
  sans,
  (s) => {
    if (s.length < moves.value.length) {
      moves.value = moves.value.slice(0, s.length);
      cursor.value = Math.min(cursor.value, s.length);
    }
  },
  { immediate: true },
);

function go(to: number) {
  cursor.value = Math.max(0, Math.min(moves.value.length, to));
}
function move(uci: string) {
  if (moves.value[cursor.value] !== uci) moves.value = [...moves.value.slice(0, cursor.value), uci];
  cursor.value++;
}
function reset(fen = "startpos", line: string[] = []) {
  start.value = fen;
  moves.value = line;
  cursor.value = line.length;
}

// The address follows the board, so a position can be bookmarked or shared.
watch([start, moves, cursor], () => {
  const query: Record<string, string> = {};
  if (start.value !== "startpos") query.fen = start.value;
  if (cursor.value) query.moves = moves.value.slice(0, cursor.value).join(",");
  if (build.value) query.build = build.value;
  router.replace({ query });
});

const orientation = stored<"white" | "black">("analysis.orientation", "white");
const flip = () => (orientation.value = orientation.value === "white" ? "black" : "white");

// The engine: the newest build of main unless one is chosen.
const { state: builds } = useBuilds();
const { state: status } = useStatus();
const build = ref(typeof route.query.build === "string" ? route.query.build : "");
watch(
  () => builds.value.data,
  (list) => {
    if (!list?.length) return;
    if (!list.some((b) => String(b.id) === build.value)) build.value = String(defaultBuild(list)!.id);
  },
  { immediate: true },
);
const buildOptions = computed(() =>
  (builds.value.data ?? []).map((b) => ({
    value: String(b.id),
    label: `${b.ref} · ${b.commit.slice(0, 7)}`,
    description: b.subject,
  })),
);

const engineOn = stored("analysis.engine", true);
const multipv = stored("analysis.lines", "3");

// The live search: its session, and the newest report of each line.
const session = ref<number | null>(null);
const lines = ref<Info[]>([]);
const error = ref("");
let wanted = 0;

useRunnerEvent("analysis.info", (data) => {
  const { session: s, info } = data as { session: number; info: Info };
  if (s !== session.value) return;
  const next = [...lines.value];
  next[info.multipv - 1] = info;
  lines.value = next;
  if (info.multipv === 1) remember(position.value.fen, info);
});
useRunnerEvent("analysis.stopped", (data) => {
  if ((data as { session: number }).session === session.value) session.value = null;
});

async function start_() {
  const ticket = ++wanted;
  error.value = "";
  lines.value = [];
  session.value = null;
  try {
    const threads = Math.max(1, Math.floor((status.value.data?.cpu.threads ?? 2) / 2));
    const live = await unwrap(
      api.POST("/api/analysis", {
        body: {
          build: build.value ? Number(build.value) : null,
          fen: start.value,
          moves: position.value.moves,
          multipv: Number(multipv.value),
          threads,
          hash: 256,
          limits: {},
        },
      }),
    );
    // A newer request may have started while this one was on its way.
    if (ticket === wanted) session.value = live.session;
  } catch (e) {
    if (ticket === wanted) error.value = (e as Error).message;
  }
}

async function stop() {
  wanted++;
  if (session.value === null) return;
  session.value = null;
  await api.DELETE("/api/analysis");
}

// Any change to what is analysed restarts the search, a moment later so stepping through moves does not flood it.
let timer: ReturnType<typeof setTimeout> | undefined;
watch(
  [() => position.value.fen, build, multipv, engineOn],
  () => {
    clearTimeout(timer);
    if (!engineOn.value || !build.value) {
      stop();
      return;
    }
    timer = setTimeout(start_, 120);
  },
  { immediate: true },
);
onBeforeUnmount(() => {
  clearTimeout(timer);
  stop();
});

// The best evaluation seen for each position, so the bar and the graph keep what the engine found.
const evals = reactive(new Map<string, { pawns: number; text: string; depth: number }>());
function remember(fen: string, info: Info) {
  const turn = fen.split(" ")[1] === "b" ? "black" : "white";
  const known = evals.get(fen);
  if (known && known.depth > info.depth) return;
  evals.set(fen, { pawns: whitePawns(info.score, turn), text: whiteScore(info.score, turn), depth: info.depth });
}

const current = computed(() => evals.get(position.value.fen));
const best = computed(() => lines.value[0]);
const shown = computed(() => lines.value.filter(Boolean));

const shapes = computed<DrawShape[]>(() =>
  shown.value
    .map((l, i) => ({
      orig: l.pv[0]!.slice(0, 2) as Key,
      dest: l.pv[0]!.slice(2, 4) as Key,
      brush: i === 0 ? "green" : "paleGreen",
      modifiers: { lineWidth: i === 0 ? 12 : 7 },
    }))
    .reverse(),
);

/** The position after each ply, for the graph. */
const fens = computed(() => {
  const out: string[] = [];
  for (let i = 0; i <= moves.value.length; i++)
    out.push(play({ fen: start.value, moves: moves.value.slice(0, i) })!.fen);
  return out;
});
const graph = computed(() =>
  fens.value.map((f) => {
    const e = evals.get(f);
    return e ? Math.max(-8, Math.min(8, e.pawns)) : null;
  }),
);

/** Move pairs for the list: `1. e4 e5`. */
const rows = computed(() => {
  const offset = position.value && start.value !== "startpos" && start.value.split(" ")[1] === "b" ? 1 : 0;
  const firstNumber = start.value === "startpos" ? 1 : Number(start.value.split(" ")[5] ?? 1);
  const out: { number: number; plies: ({ san: string; ply: number } | null)[] }[] = [];
  const all: ({ san: string; ply: number } | null)[] = [
    ...Array(offset).fill(null),
    ...sans.value.map((san, i) => ({ san, ply: i + 1 })),
  ];
  for (let i = 0; i < all.length; i += 2)
    out.push({ number: firstNumber + i / 2, plies: [all[i] ?? null, all[i + 1] ?? null] });
  return out;
});

function playLine(info: Info) {
  if (info.pv[0]) move(info.pv[0]);
}

// Loading a FEN or a PGN.
const importOpen = ref(false);
const importText = ref("");
const importError = ref("");
function load() {
  const line = readPosition(importText.value);
  if (!line) {
    importError.value = "That is not a FEN or a PGN with moves.";
    return;
  }
  reset(line.fen, line.moves);
  importOpen.value = false;
  importText.value = "";
  importError.value = "";
}

const moveList = ref<HTMLElement>();
followCurrent(moveList, cursor);

const pgn = () => toPgn({ fen: start.value, moves: moves.value.slice(0, cursor.value) });

function onKey(e: KeyboardEvent) {
  const target = e.target as HTMLElement | null;
  if (target?.closest("input, textarea, [contenteditable], [role=dialog]")) return;
  if (e.key === "ArrowLeft") go(cursor.value - 1);
  else if (e.key === "ArrowRight") go(cursor.value + 1);
  else if (e.key === "Home") go(0);
  else if (e.key === "End") go(Infinity);
  else return;
  e.preventDefault();
}
onMounted(() => window.addEventListener("keydown", onKey));
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
useShortcuts({ f: flip, e: () => (engineOn.value = !engineOn.value) });

const noBuilds = computed(() => builds.value.status === "success" && builds.value.data.length === 0);
</script>

<template>
  <div class="page">
    <EPageHeader
      title="Analysis"
      description="Explore any position with the engine. Move pieces, step through a game or paste one in."
    >
      <template #actions>
        <EButton variant="outline" :icon="PhUploadSimple" @click="importOpen = true">Paste a game</EButton>
        <EButton variant="quiet" :icon="PhArrowCounterClockwise" @click="reset()">New board</EButton>
      </template>
    </EPageHeader>

    <div class="analysis">
      <section class="board-col" aria-label="Board">
        <div class="board-row">
          <EvalBar :pawns="current?.pawns" :text="current?.text" :orientation="orientation" />
          <ChessBoard
            :position="position"
            :orientation="orientation"
            movable
            :shapes="engineOn ? shapes : []"
            @move="move"
          />
        </div>
        <div class="board-controls">
          <EInline gap="tight">
            <EIconButton :icon="PhCaretDoubleLeft" label="First move" tooltip :disabled="cursor === 0" @click="go(0)" />
            <EIconButton :icon="PhCaretLeft" label="Back" tooltip :disabled="cursor === 0" @click="go(cursor - 1)" />
            <EIconButton
              :icon="PhCaretRight"
              label="Forward"
              tooltip
              :disabled="cursor === moves.length"
              @click="go(cursor + 1)"
            />
            <EIconButton
              :icon="PhCaretDoubleRight"
              label="Last move"
              tooltip
              :disabled="cursor === moves.length"
              @click="go(Infinity)"
            />
          </EInline>
          <p class="keys">
            <EKbd size="sm">←</EKbd><EKbd size="sm">→</EKbd> step · <EKbd size="sm">F</EKbd> flip ·
            <EKbd size="sm">E</EKbd> engine
          </p>
          <EIconButton :icon="PhArrowsDownUp" label="Flip board" tooltip @click="flip" />
        </div>
      </section>

      <div class="side">
        <section class="panel engine" aria-label="Engine">
          <header class="panel-head">
            <ESwitch v-model="engineOn" label="Engine" />
            <ESelect
              v-if="buildOptions.length"
              v-model="build"
              label="Engine version"
              hide-label
              size="sm"
              :options="buildOptions"
              class="engine-pick"
            />
          </header>

          <EEmptyState v-if="noBuilds" title="No engine built yet" icon="info" size="sm" bare>
            Build Reckless once and it analyses here.
            <template #action>
              <EButton size="sm" @click="router.push('/engines')">Build an engine</EButton>
            </template>
          </EEmptyState>

          <template v-else>
            <div class="hero">
              <div class="hero-eval num">
                <template v-if="current">{{ current.text }}</template>
                <template v-else-if="engineOn">…</template>
                <template v-else>–</template>
              </div>
              <div class="hero-text">
                <p v-if="!engineOn" class="hero-best">Engine off</p>
                <p v-else-if="best" class="hero-best">
                  Best move
                  <strong>{{ sanLine(position.fen, best.pv.slice(0, 1)).replace(/^\d+\.(\.\.)?\s*/, "") }}</strong>
                </p>
                <p v-else class="hero-best">Thinking…</p>
                <p v-if="best && engineOn" class="hero-meta num">
                  <Term of="depth" inline>depth {{ best.depth }}/{{ best.seldepth }}</Term> ·
                  {{ (best.nodes / 1e6).toFixed(1) }}M positions · {{ speed(best.nps) }}
                </p>
              </div>
            </div>

            <ENotice v-if="error" tone="danger" title="The engine could not start" :role="false">{{ error }}</ENotice>

            <ol v-if="engineOn" class="lines" aria-label="Engine lines">
              <li v-for="l in shown" :key="l.multipv" class="line">
                <span class="pill num" :class="whitePawns(l.score, position.turn) >= 0 ? 'pill--white' : 'pill--black'">
                  {{ whiteScore(l.score, position.turn) }}
                </span>
                <span class="line-moves">{{ sanLine(position.fen, l.pv.slice(0, 12)) }}</span>
                <EIconButton :icon="PhPlay" size="sm" label="Play this move" tooltip @click="playLine(l)" />
              </li>
              <template v-if="!shown.length && !error">
                <li v-for="i in Number(multipv)" :key="i" class="line"><ESkeleton text /></li>
              </template>
            </ol>

            <ESegmented v-model="multipv" label="Lines to show" size="sm" :options="['1', '2', '3', '5']" />
          </template>
        </section>

        <section class="panel" aria-label="Moves">
          <header class="panel-head">
            <h2 class="panel-title">Moves</h2>
            <EInline gap="tight">
              <ECopyButton :text="() => position.fen" label="Copy FEN" size="sm" />
              <ECopyButton :text="pgn" label="Copy PGN" size="sm" />
            </EInline>
          </header>
          <div ref="moveList" class="moves" role="list">
            <EButton
              role="listitem"
              size="sm"
              variant="quiet"
              :aria-current="cursor === 0 ? 'step' : undefined"
              class="move-start"
              @click="go(0)"
            >
              Start
            </EButton>
            <template v-for="r in rows" :key="r.number">
              <span class="move-number num" role="presentation">{{ r.number }}.</span>
              <template v-for="(p, i) in r.plies" :key="i">
                <EButton
                  v-if="p"
                  role="listitem"
                  size="sm"
                  variant="quiet"
                  :aria-current="p.ply === cursor ? 'step' : undefined"
                  :class="{ 'move-future': p.ply > cursor }"
                  @click="go(p.ply)"
                >
                  {{ p.san }}
                </EButton>
                <span v-else class="move-gap">…</span>
              </template>
            </template>
          </div>
          <p v-if="!moves.length" class="quiet">Make a move on the board, or paste a game.</p>
        </section>

        <section v-if="moves.length" class="panel" aria-label="Evaluation graph">
          <h2 class="panel-title"><Term of="eval">Evaluation</Term> by move</h2>
          <LineChart
            label="Evaluation from White's side by move"
            :x="fens.map((_, i) => i)"
            :series="[{ label: 'eval', values: graph, fill: true }]"
            :range="[-8, 8]"
            :legend="false"
            :height="120"
            :marker="cursor"
            @select="go"
          />
          <p class="quiet">Click to jump. Positions fill in as the engine looks at them.</p>
        </section>
      </div>
    </div>

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
  </div>
</template>

<style scoped>
.page {
  display: grid;
  gap: var(--e-gap-section);
}

.analysis {
  display: grid;

  /* The board is as large as the window allows: never taller than the space under the header. */
  --board: min(calc(100dvh - var(--e-header-height, 4rem) - 12rem), 46rem);

  grid-template-columns: minmax(18rem, var(--board)) minmax(20rem, 1fr);
  gap: var(--e-gap-loose);
  align-items: start;
}

/* The board is as large as the window allows: the full column, but never taller than the space under the header. */
.board-col {
  display: grid;
  gap: var(--e-gap-snug);
  min-width: 0;
}

.board-row {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  gap: var(--e-gap-snug);
}

.board-controls {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--e-gap-base);
  padding-inline-start: calc(1.75rem + var(--e-gap-snug));
}

.keys {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.side {
  display: grid;
  gap: var(--e-gap-base);
  min-width: 0;
}

.panel {
  display: grid;
  gap: var(--e-gap-base);
  padding: var(--e-pad-card);
  border-radius: var(--e-radius-lg);
  background: var(--e-surface);
  box-shadow: var(--e-shadow-frame);
}

.panel-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--e-gap-base);
}

.panel-title {
  margin: 0;
  font: var(--e-type-h5);
}

.engine-pick {
  flex: 1;
  max-width: 16rem;
}

.hero {
  display: flex;
  align-items: center;
  gap: var(--e-gap-loose);
  padding-block: var(--e-gap-snug);
  border-block: 1px solid var(--e-border);
  border-color: color-mix(in oklab, var(--e-border) 40%, transparent);
}

.hero-eval {
  min-width: 5.5rem;
  font: 600 var(--e-text-3xl) / 1 var(--e-font-mono);
  letter-spacing: -0.03em;
}

.hero-text {
  display: grid;
  gap: 0.25rem;
  min-width: 0;
}

.hero-best,
.hero-meta {
  margin: 0;
}

.hero-best strong {
  color: var(--e-primary);
  font: 600 var(--e-text-lg) / 1 var(--e-font-mono);
}

.hero-meta {
  color: var(--e-muted);
  font: var(--e-type-small);
}

.lines {
  display: grid;
  gap: var(--e-gap-tight);
  margin: 0;
  padding: 0;
  list-style: none;
}

.line {
  display: grid;
  grid-template-columns: 4.25rem minmax(0, 1fr) auto;
  align-items: center;
  gap: var(--e-gap-snug);
  min-height: 2.25rem;
}

.pill {
  padding: 0.2rem 0.4rem;
  border-radius: var(--e-radius-sm);
  font: 600 var(--e-text-sm) / 1.2 var(--e-font-mono);
  text-align: center;
}

.pill--white {
  background: #f4ede2;
  box-shadow: inset 0 0 0 1px rgb(0 0 0 / 12%);
  color: #2a2420;
}

.pill--black {
  background: #2a2420;
  color: #f4ede2;
}

.line-moves {
  overflow: hidden;
  font: var(--e-type-small);
  font-family: var(--e-font-mono);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.moves {
  position: relative;
  display: grid;
  grid-template-columns: 2.5rem 1fr 1fr;
  align-items: center;
  gap: 0.125rem var(--e-gap-tight);
  max-height: 16rem;
  overflow-y: auto;
  font-family: var(--e-font-mono);
}

.move-start {
  grid-column: 1 / -1;
  justify-self: start;
}

.move-number {
  color: var(--e-muted);
  font: var(--e-type-small);
  text-align: end;
}

.move-gap {
  padding-inline: var(--e-gap-snug);
  color: var(--e-muted);
}

.moves :deep(.e-btn) {
  justify-content: flex-start;
  text-decoration: none;
}

/* The move on the board: the place tint, as eos marks where you are. */
.moves :deep(.e-btn[aria-current]) {
  background: var(--e-primary-soft);
  color: var(--e-primary);
  font-weight: 600;
}

.move-future {
  opacity: 0.55;
}

.quiet {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

@media (width < 64rem) {
  .analysis {
    grid-template-columns: minmax(0, 1fr);
  }

  .board-col {
    width: min(100%, 40rem);
    margin-inline: auto;
  }

  .keys {
    display: none;
  }
}
</style>
