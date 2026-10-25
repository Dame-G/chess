<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import type { DrawShape } from "@lichess-org/chessground/draw";
import type { Key } from "@lichess-org/chessground/types";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { EForm } from "@eos-ui/vue/components/EForm";
import { EInline } from "@eos-ui/vue/components/EInline";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { EPageHeader } from "@eos-ui/vue/components/EPageHeader";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESelect } from "@eos-ui/vue/components/ESelect";
import { EStack } from "@eos-ui/vue/components/EStack";
import { ETable } from "@eos-ui/vue/components/ETable";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import ChessBoard from "../board/ChessBoard.vue";
import { isValidFen, play, sanLine, whiteScore, type Line } from "../board/position";
import { useRunnerEvent } from "../api/events";
import { api } from "../api/runner";
import { unwrap, useBuilds } from "../api/queries";
import type { components } from "../api/schema";

type Info = components["schemas"]["Info"];

const line = ref<Line>({ fen: "startpos", moves: [] });
const position = computed(() => play(line.value)!);

const { state: builds } = useBuilds();
const buildOptions = computed(() =>
  (builds.value.data ?? []).map((b) => ({ value: String(b.id), label: `${b.id}: ${b.ref} ${b.commit.slice(0, 8)}` })),
);
const build = ref("");
watch(buildOptions, (options) => {
  if (!build.value && options[0]) build.value = options[0].value;
});

const multipv = ref("1");

// The live search: its session id, and the newest report of each line.
const session = ref<number | null>(null);
const lines = ref<Info[]>([]);
const error = ref("");

useRunnerEvent("analysis.info", (data) => {
  const { session: s, info } = data as { session: number; info: Info };
  if (s !== session.value) return;
  const next = [...lines.value];
  next[info.multipv - 1] = info;
  lines.value = next;
});
useRunnerEvent("analysis.stopped", (data) => {
  if ((data as { session: number }).session === session.value) session.value = null;
});

async function start() {
  error.value = "";
  lines.value = [];
  try {
    const live = await unwrap(
      api.POST("/api/analysis", {
        body: {
          build: build.value ? Number(build.value) : null,
          fen: line.value.fen,
          moves: position.value.moves,
          multipv: Number(multipv.value),
          threads: 1,
          hash: 64,
          limits: {},
        },
      }),
    );
    session.value = live.session;
  } catch (e) {
    error.value = (e as Error).message;
    session.value = null;
  }
}

async function stop() {
  session.value = null;
  await api.DELETE("/api/analysis");
}

// A new position, build or line count restarts a running search.
watch([() => position.value.fen, build, multipv], () => {
  if (session.value !== null) start();
});
onBeforeUnmount(() => {
  if (session.value !== null) stop();
});

function move(uci: string) {
  line.value = { ...line.value, moves: [...position.value.moves, uci] };
}
function back() {
  line.value = { ...line.value, moves: position.value.moves.slice(0, -1) };
}

const fenInput = ref("");
const fenError = computed(() => (fenInput.value && !isValidFen(fenInput.value) ? "Not a legal position" : undefined));
function loadFen() {
  if (fenInput.value && !fenError.value) line.value = { fen: fenInput.value.trim(), moves: [] };
}

const shapes = computed<DrawShape[]>(() =>
  lines.value
    .filter(Boolean)
    .map((l, i) => ({
      orig: l.pv[0]!.slice(0, 2) as Key,
      dest: l.pv[0]!.slice(2, 4) as Key,
      brush: i === 0 ? "green" : "paleBlue",
    }))
    .reverse(),
);

const best = computed(() => lines.value[0]);
</script>

<template>
  <EStack gap="section">
    <EPageHeader title="Analysis" description="Move on the board; the engine follows the position." />

    <EEmptyState v-if="builds.data?.length === 0" title="No builds yet" icon="info">
      Build an engine on the Engines page first.
    </EEmptyState>

    <div v-else class="analysis">
      <EStack gap="base">
        <ChessBoard :position="position" movable :shapes="shapes" @move="move" />
        <EInline gap="snug">
          <EButton variant="outline" size="sm" :disabled="!position.moves.length" @click="back">Back</EButton>
          <EButton variant="outline" size="sm" @click="line = { fen: 'startpos', moves: [] }">Start position</EButton>
        </EInline>
      </EStack>

      <EStack gap="base">
        <ESelect v-model="build" label="Build" :options="buildOptions" :loading="builds.status === 'pending'" />
        <ESegmented v-model="multipv" label="Lines" :options="['1', '2', '3', '5']" size="sm" />
        <EInline gap="snug">
          <EButton v-if="session === null" @click="start">Analyse</EButton>
          <EButton v-else variant="outline" @click="stop">Stop</EButton>
        </EInline>
        <ENotice v-if="error" tone="danger" title="Could not analyse">{{ error }}</ENotice>

        <ETable v-if="best" caption="Engine lines" dense>
          <thead>
            <tr>
              <th scope="col" class="e-num">Eval</th>
              <th scope="col">Line</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="l in lines.filter(Boolean)" :key="l.multipv">
              <th scope="row" class="e-num">{{ whiteScore(l.score, position.turn) }}</th>
              <td>{{ sanLine(position.fen, l.pv) }}</td>
            </tr>
          </tbody>
        </ETable>
        <p v-if="best" class="analysis-meta">
          Depth {{ best.depth }}/{{ best.seldepth }} · {{ (best.nodes / 1e6).toFixed(1) }}M nodes ·
          {{ (best.nps / 1000).toFixed(0) }}k nps
        </p>

        <EForm @submit="loadFen">
          <ETextField v-model="fenInput" label="Load a FEN" :error="fenError" size="sm" />
          <EButton type="submit" variant="outline" size="sm">Load</EButton>
        </EForm>
        <p class="analysis-meta">
          <code>{{ position.fen }}</code>
        </p>
      </EStack>
    </div>
  </EStack>
</template>

<style scoped>
.analysis {
  display: grid;
  grid-template-columns: minmax(0, 40rem) minmax(16rem, 1fr);
  gap: var(--e-gap-loose);
  align-items: start;
}

@media (width < 60rem) {
  .analysis {
    grid-template-columns: minmax(0, 1fr);
  }
}

.analysis-meta {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small, inherit);
  overflow-wrap: anywhere;
}
</style>
