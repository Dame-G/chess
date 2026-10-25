<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { RouterLink, useRoute, useRouter } from "vue-router";
import { useQuery } from "@pinia/colada";
import {
  PhArrowsDownUp,
  PhCaretDoubleLeft,
  PhCaretDoubleRight,
  PhCaretLeft,
  PhCaretRight,
  PhChartLine,
} from "@phosphor-icons/vue";
import { EBreadcrumb } from "@eos-ui/vue/components/EBreadcrumb";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ECopyButton } from "@eos-ui/vue/components/ECopyButton";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";
import { EInline } from "@eos-ui/vue/components/EInline";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { EPageHeader } from "@eos-ui/vue/components/EPageHeader";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import ChessBoard from "../board/ChessBoard.vue";
import { followCurrent } from "../board/follow";
import EvalBar from "../board/EvalBar.vue";
import LineChart from "../charts/LineChart.vue";
import Term from "../components/Term.vue";
import { readGame } from "../board/game";
import { play } from "../board/position";
import { api } from "../api/runner";
import { unwrap, useExperiment, useTest } from "../api/queries";
import { ending, signed, stageLabel } from "../format";

const route = useRoute();
const router = useRouter();
const testId = computed(() => Number(route.params.id));
const index = computed(() => Number(route.params.index));

const { state } = useQuery({
  key: () => ["game", testId.value, index.value],
  query: () =>
    unwrap(
      api.GET("/api/tests/{id}/games/{index}", {
        params: { path: { id: testId.value, index: index.value } },
        parseAs: "text",
      }),
    ),
});
const { state: testState } = useTest(testId);
const test = computed(() => testState.value.data?.test);
const { state: experimentState } = useExperiment(() => test.value?.experiment ?? 0);
const experiment = computed(() => experimentState.value.data?.experiment);

const game = computed(() => (state.value.data ? readGame(state.value.data) : undefined));

/** How many plies are shown: 0 is the starting position. */
const ply = ref(0);
watch(game, (g) => (ply.value = g?.plies.length ?? 0), { immediate: true });

const devWhite = computed(() => game.value?.headers.White === "dev");
const orientation = ref<"white" | "black">("white");
watch(devWhite, (w) => (orientation.value = w ? "white" : "black"), { immediate: true });

const position = computed(() => {
  const g = game.value;
  if (!g) return undefined;
  return play({ fen: g.startFen, moves: g.plies.slice(0, ply.value).map((p) => p.uci) });
});
const current = computed(() => (ply.value > 0 ? game.value?.plies[ply.value - 1] : undefined));

/** The last evaluation at or before the shown move, for the bar. */
const shownEval = computed(() => {
  const plies = game.value?.plies.slice(0, ply.value) ?? [];
  for (let i = plies.length - 1; i >= 0; i--) if (plies[i]!.eval !== null) return plies[i]!.eval!;
  return undefined;
});
const evalText = (e: number) => (Math.abs(e) >= 100 ? (e > 0 ? "#" : "#-") : signed(e));

const moveList = ref<HTMLElement>();
followCurrent(moveList, ply);

function go(to: number) {
  ply.value = Math.max(0, Math.min(game.value?.plies.length ?? 0, to));
}
function onKey(e: KeyboardEvent) {
  if ((e.target as HTMLElement | null)?.closest("input, textarea, [role=dialog]")) return;
  if (e.key === "ArrowLeft") go(ply.value - 1);
  else if (e.key === "ArrowRight") go(ply.value + 1);
  else if (e.key === "Home") go(0);
  else if (e.key === "End") go(Infinity);
  else if (e.key === "f") orientation.value = orientation.value === "white" ? "black" : "white";
  else return;
  e.preventDefault();
}
onMounted(() => window.addEventListener("keydown", onKey));
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));

/** Evaluations clamped to ±8 pawns, so one mate does not flatten the graph. */
const evals = computed(
  () => game.value?.plies.map((p) => (p.eval === null ? null : Math.max(-8, Math.min(8, p.eval)))) ?? [],
);

const result = computed(() => {
  const r = game.value?.headers.Result;
  if (r === "1/2-1/2") return { label: "Draw", tone: "neutral" } as const;
  if (!r || r === "*") return { label: "Unfinished", tone: "neutral" } as const;
  return (r === "1-0") === devWhite.value
    ? ({ label: "New version won", tone: "success" } as const)
    : ({ label: "New version lost", tone: "danger" } as const);
});

const rows = computed(() => {
  const plies = game.value?.plies ?? [];
  const blackFirst = game.value?.startFen.split(" ")[1] === "b";
  const all = [...(blackFirst ? [null] : []), ...plies.map((p, i) => ({ ...p, ply: i + 1 }))];
  const firstNumber = Number(game.value?.startFen.split(" ")[5] ?? 1);
  const out = [];
  for (let i = 0; i < all.length; i += 2)
    out.push({ number: firstNumber + i / 2, plies: [all[i] ?? null, all[i + 1] ?? null] });
  return out;
});

function analyse() {
  const g = game.value;
  if (!g) return;
  const query: Record<string, string> = {
    fen: g.startFen,
    moves: g.plies
      .slice(0, ply.value)
      .map((p) => p.uci)
      .join(","),
  };
  if (test.value) query.build = String(test.value.dev_build);
  router.push({ path: "/analysis", query });
}

const name = (side: string | undefined) =>
  side === "dev" ? "New version" : side === "base" ? "Current version" : (side ?? "");
</script>

<template>
  <div class="page">
    <EPageHeader
      :title="`Game ${index + 1}`"
      :description="test ? `${stageLabel(test.stage)}${experiment ? ` · ${experiment.name}` : ''}` : undefined"
    >
      <template #breadcrumb>
        <EBreadcrumb
          :link-component="RouterLink"
          :items="[
            { label: 'Experiments', to: '/experiments' },
            ...(experiment ? [{ label: experiment.name, to: `/experiments/${experiment.id}` }] : []),
            { label: `Game ${index + 1}` },
          ]"
        />
      </template>
      <template v-if="game" #status><EStamp :tone="result.tone" :label="result.label" /></template>
      <template #actions>
        <EButton variant="outline" :icon="PhChartLine" :disabled="!game" @click="analyse">Analyse from here</EButton>
      </template>
    </EPageHeader>

    <ENotice v-if="state.status === 'error'" tone="danger" title="Could not load the game">
      {{ state.error?.message }}
    </ENotice>
    <ESkeleton v-else-if="!game || !position" :height="480" label="Loading the game" />

    <div v-else class="game">
      <section class="board-col" aria-label="Board">
        <p class="player">
          <span :class="['side', orientation === 'white' ? 'side--black' : 'side--white']" aria-hidden="true"></span>
          {{ name(orientation === "white" ? game.headers.Black : game.headers.White) }}
        </p>
        <div class="board-row">
          <EvalBar
            :pawns="shownEval"
            :text="shownEval !== undefined ? evalText(shownEval) : undefined"
            :orientation="orientation"
          />
          <ChessBoard :position="position" :orientation="orientation" />
        </div>
        <p class="player">
          <span :class="['side', orientation === 'white' ? 'side--white' : 'side--black']" aria-hidden="true"></span>
          {{ name(orientation === "white" ? game.headers.White : game.headers.Black) }}
        </p>
        <div class="board-controls">
          <EInline gap="tight">
            <EIconButton :icon="PhCaretDoubleLeft" label="First move" tooltip :disabled="ply === 0" @click="go(0)" />
            <EIconButton :icon="PhCaretLeft" label="Back" tooltip :disabled="ply === 0" @click="go(ply - 1)" />
            <EIconButton
              :icon="PhCaretRight"
              label="Forward"
              tooltip
              :disabled="ply === game.plies.length"
              @click="go(ply + 1)"
            />
            <EIconButton
              :icon="PhCaretDoubleRight"
              label="Last move"
              tooltip
              :disabled="ply === game.plies.length"
              @click="go(Infinity)"
            />
          </EInline>
          <EIconButton
            :icon="PhArrowsDownUp"
            label="Flip board"
            tooltip
            @click="orientation = orientation === 'white' ? 'black' : 'white'"
          />
        </div>
      </section>

      <div class="side-col">
        <section class="panel" aria-label="This move">
          <template v-if="current">
            <p class="move-title">
              {{ Math.ceil(ply / 2) }}{{ ply % 2 ? "." : "…" }} <strong>{{ current.san }}</strong>
            </p>
            <dl class="facts">
              <div v-if="current.eval !== null">
                <dt><Term of="eval" inline /></dt>
                <dd class="num">{{ evalText(current.eval) }}</dd>
              </div>
              <div v-if="current.depth !== null">
                <dt><Term of="depth" inline /></dt>
                <dd class="num">{{ current.depth }}</dd>
              </div>
              <div v-if="current.nodes !== null">
                <dt>Positions</dt>
                <dd class="num">{{ current.nodes.toLocaleString() }}</dd>
              </div>
              <div v-if="current.time !== null">
                <dt>Time</dt>
                <dd class="num">{{ current.time.toFixed(2) }}s</dd>
              </div>
            </dl>
          </template>
          <p v-else class="quiet">
            Starting position{{ game.headers.FEN ? " from the opening book" : "" }}. Step forward to follow the game.
          </p>
          <p class="quiet">
            {{ ending(game.headers.Termination) }} after {{ Math.ceil(game.plies.length / 2) }} moves.
          </p>
        </section>

        <section class="panel" aria-label="Evaluation graph">
          <h2 class="panel-title"><Term of="eval">Evaluation</Term> by move</h2>
          <LineChart
            label="Evaluation from White's side by move"
            :x="game.plies.map((_, i) => i + 1)"
            :series="[{ label: 'eval', values: evals, fill: true }]"
            :range="[-8, 8]"
            :legend="false"
            :height="130"
            :marker="ply - 1"
            @select="(i) => go(i + 1)"
          />
        </section>

        <section class="panel" aria-label="Moves">
          <header class="panel-head">
            <h2 class="panel-title">Moves</h2>
            <ECopyButton :text="() => state.data ?? ''" label="Copy PGN" size="sm" />
          </header>
          <div ref="moveList" class="moves" role="list">
            <template v-for="r in rows" :key="r.number">
              <span class="move-number num" role="presentation">{{ r.number }}.</span>
              <template v-for="(p, i) in r.plies" :key="i">
                <EButton
                  v-if="p"
                  role="listitem"
                  size="sm"
                  variant="quiet"
                  :aria-current="p.ply === ply ? 'step' : undefined"
                  @click="go(p.ply)"
                >
                  {{ p.san }}
                </EButton>
                <span v-else class="move-gap">…</span>
              </template>
            </template>
          </div>
        </section>
      </div>
    </div>
  </div>
</template>

<style scoped>
.page {
  display: grid;
  gap: var(--e-gap-section);
}

.game {
  display: grid;

  /* The board is as large as the window allows: never taller than the space under the header. */
  --board: min(calc(100dvh - var(--e-header-height, 4rem) - 15rem), 46rem);

  grid-template-columns: minmax(18rem, var(--board)) minmax(20rem, 1fr);
  gap: var(--e-gap-loose);
  align-items: start;
}

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

.player {
  display: flex;
  align-items: center;
  gap: var(--e-gap-snug);
  margin: 0;
  padding-inline-start: calc(1.75rem + var(--e-gap-snug));
  font: var(--e-type-h6);
}

.side {
  width: 0.75rem;
  height: 0.75rem;
  border-radius: 50%;
  box-shadow: 0 0 0 1px var(--e-border-strong);
}

.side--white {
  background: #fffaf3;
}

.side--black {
  background: #2a2420;
}

.board-controls {
  display: flex;
  justify-content: space-between;
  padding-inline-start: calc(1.75rem + var(--e-gap-snug));
}

.side-col {
  display: grid;
  gap: var(--e-gap-base);
  min-width: 0;
}

.panel {
  display: grid;
  gap: var(--e-gap-snug);
  padding: var(--e-pad-card);
  border-radius: var(--e-radius-lg);
  background: var(--e-surface);
  box-shadow: var(--e-shadow-frame);
}

.panel-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.panel-title {
  margin: 0;
  font: var(--e-type-h5);
}

.move-title {
  margin: 0;
  font: var(--e-type-h4);
}

.move-title strong {
  color: var(--e-primary);
  font-family: var(--e-font-mono);
}

.facts {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(6rem, 1fr));
  gap: var(--e-gap-snug);
  margin: 0;
}

.facts dt {
  color: var(--e-muted);
  font: var(--e-type-small);
}

.facts dd {
  margin: 0;
  font: 600 var(--e-text-lg) / 1.3 var(--e-font-mono);
}

.moves {
  position: relative;
  display: grid;
  grid-template-columns: 2.5rem 1fr 1fr;
  align-items: center;
  gap: 0.125rem var(--e-gap-tight);
  max-height: 18rem;
  overflow-y: auto;
  font-family: var(--e-font-mono);
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

.move-number {
  color: var(--e-muted);
  font: var(--e-type-small);
  text-align: end;
}

.move-gap {
  padding-inline: var(--e-gap-snug);
  color: var(--e-muted);
}

.quiet {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

@media (width < 64rem) {
  .game {
    grid-template-columns: minmax(0, 1fr);
  }

  .board-col {
    width: min(100%, 40rem);
    margin-inline: auto;
  }
}
</style>
