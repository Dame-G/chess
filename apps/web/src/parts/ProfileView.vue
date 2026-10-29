<script setup lang="ts">
import { computed, ref } from "vue";
import { RouterLink } from "vue-router";
import { ELink } from "@eos-ui/vue/components/ELink";
import "../board/review.css";
import type { Classification, GameId, Moment, Profile, ProgressChart as ProgressData } from "../api/types";
import { markOf } from "../board/review";
import { formatDate, formatNumber, formatPercent, plural, resultText, speedName } from "../format";
import UiSection from "../ui/UiSection.vue";
import UiSeg from "../ui/UiSeg.vue";
import UiStat from "../ui/UiStat.vue";
import UiStats from "../ui/UiStats.vue";
import UiTable from "../ui/UiTable.vue";
import ProfileMeter from "./ProfileMeter.vue";
import ProgressChart from "./ProgressChart.vue";
import TrendChart from "./TrendChart.vue";
import WdlBar from "./WdlBar.vue";

/**
 * A player's games added up, the same building blocks for You and anyone on Lichess: the trend, results, accuracy by
 * phase and by speed, moves per game, time per move, and the best moments and worst blunders (links onto the board).
 * What needs a review is left out until there is one. With `progress` (yours) the trend can also be read beside
 * Dameless's.
 */
const props = defineProps<{ profile: Profile; progress?: ProgressData | null }>();

const reviewed = computed(() => props.profile.reviewed > 0);

// ---- Progress
const view = ref<"games" | "dameless">("games");
const trendShown = computed(() => props.profile.trend.length > 1);
const withDameless = computed(() => !!props.progress && props.progress.series.some((s) => s.points.length > 1));
const progressView = computed(() => view.value === "dameless" && withDameless.value);

// ---- Results
const sides = computed(() => [
  { id: "all", label: "All games", wdl: props.profile.results.overall },
  { id: "white", label: "As White", wdl: props.profile.results.white },
  { id: "black", label: "As Black", wdl: props.profile.results.black },
]);
const resultColumns = [
  { key: "label", label: "Side" },
  { key: "bar", label: "Won · drawn · lost", width: "55%" },
  { key: "games", label: "Games", fig: true, hide: "sm" as const },
  { key: "score", label: "Score", fig: true },
];

// ---- Accuracy
const phases = computed(() => [
  { id: "opening", label: "Opening", value: props.profile.accuracy.opening },
  { id: "middlegame", label: "Middlegame", value: props.profile.accuracy.middlegame },
  { id: "endgame", label: "Endgame", value: props.profile.accuracy.endgame },
]);
const speeds = computed(() =>
  props.profile.accuracy.by_speed
    .filter((s) => s.speed && s.accuracy != null)
    .map((s) => ({ id: s.speed!, label: speedName[s.speed!], value: s.accuracy, games: s.reviewed })),
);
const phaseColumns = [
  { key: "label", label: "By phase" },
  { key: "bar", label: "", width: "40%" },
  { key: "value", label: "Accuracy", fig: true },
];
const speedColumns = [
  { key: "label", label: "By speed" },
  { key: "bar", label: "", width: "30%" },
  { key: "games", label: "Games", fig: true, hide: "sm" as const },
  { key: "value", label: "Accuracy", fig: true },
];

// ---- Moves per game
const CLASS_ROWS: Classification[] = [
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
const classRows = computed(() =>
  CLASS_ROWS.map((c) => ({
    id: c,
    label: markOf(c).label,
    glyph: markOf(c).glyph,
    tone: markOf(c).tone,
    perGame: props.profile.counts_per_game[c],
    total: props.profile.counts[c],
  })),
);
const classColumns = [
  { key: "label", label: "Move" },
  { key: "perGame", label: "Per game", fig: true },
  { key: "total", label: "Total", fig: true },
];

// ---- Time
const seconds = (ms: number | null | undefined) => (ms == null ? null : `${(ms / 1000).toFixed(1)}`);

// ---- Moments
const boardTo = (game: GameId, ply?: number) => ({
  path: "/",
  query: { game: String(game), ...(ply ? { ply: String(ply) } : {}) },
});
const momentText = (m: Moment) => `${Math.ceil(m.ply / 2)}${m.side === "white" ? "." : "…"} ${m.san}`;
const swing = (m: Moment) => `${m.win_before.toFixed(0)}% → ${m.win_after.toFixed(0)}%`;

interface MomentRow {
  id: string;
  mark: string;
  tone: string;
  link: string;
  to: ReturnType<typeof boardTo>;
  mono: boolean;
  note: string;
  swing: string;
}
const moments = computed<MomentRow[]>(() => {
  const m = props.profile.moments;
  const best = m.best.map((x) => ({
    id: `b${x.game}-${x.ply}`,
    mark: markOf(x.classification).glyph,
    tone: x.classification,
    link: momentText(x),
    to: boardTo(x.game, x.ply),
    mono: true,
    note: `${markOf(x.classification).label} against ${x.opponent}${x.sacrifice >= 1 ? `, ${x.sacrifice} pawns given up` : ""}`,
    swing: swing(x),
  }));
  const blunders = m.blunders.map((x) => ({
    id: `x${x.game}-${x.ply}`,
    mark: markOf(x.classification).glyph,
    tone: x.classification,
    link: momentText(x),
    to: boardTo(x.game, x.ply),
    mono: true,
    note: `Blunder against ${x.opponent}`,
    swing: swing(x),
  }));
  const games = [
    m.most_accurate_game ? { label: "Most accurate game", g: m.most_accurate_game } : null,
    m.least_accurate_game ? { label: "Least accurate game", g: m.least_accurate_game } : null,
  ]
    .filter((x) => x !== null)
    .map((x) => ({
      id: x.label,
      mark: "",
      tone: "",
      link: x.label,
      to: boardTo(x.g.game),
      mono: false,
      note: `${resultText(x.g.outcome)} against ${x.g.opponent}, ${formatDate(x.g.played_at)}`,
      swing: formatPercent(x.g.accuracy),
    }));
  return [...best, ...blunders, ...games];
});
const momentColumns = [
  { key: "link", label: "Moment" },
  { key: "note", label: "What happened", hide: "sm" as const },
  { key: "swing", label: "Win chance", fig: true },
];
</script>

<template>
  <div class="profile">
    <UiSection v-if="trendShown || withDameless" title="Progress">
      <template #aside>
        <UiSeg
          v-if="withDameless && trendShown"
          v-model="view"
          label="Progress chart"
          size="sm"
          :options="[
            { value: 'games', label: 'Your games' },
            { value: 'dameless', label: 'With Dameless' },
          ]"
        />
      </template>
      <ProgressChart v-if="progress && (progressView || !trendShown)" :chart="progress" />
      <TrendChart
        v-else
        :points="profile.trend"
        :label="`Accuracy and rating over the last ${profile.trend.length} games`"
      />
    </UiSection>

    <UiSection v-if="profile.games" title="Results">
      <template v-if="profile.opponents.average_rating" #note>
        Against opponents averaging {{ formatNumber(profile.opponents.average_rating) }}
      </template>
      <UiTable label="Results" :columns="resultColumns" :rows="sides" row-key="id">
        <template #cell-bar="{ row }"><WdlBar :wdl="row.wdl" :label="row.label" counts /></template>
        <template #cell-games="{ row }">{{ row.wdl.games }}</template>
        <template #cell-score="{ row }">{{ formatPercent(row.wdl.score) }}</template>
      </UiTable>
    </UiSection>

    <UiSection v-if="reviewed" title="Accuracy">
      <template #note>Over {{ plural(profile.reviewed, "reviewed game") }}</template>
      <div class="pair">
        <UiTable label="Accuracy by phase" :columns="phaseColumns" :rows="phases" row-key="id">
          <template #cell-bar="{ row }"><ProfileMeter :value="row.value" :label="row.label" /></template>
          <template #cell-value="{ row }">{{ formatPercent(row.value) }}</template>
        </UiTable>
        <UiTable v-if="speeds.length" label="Accuracy by speed" :columns="speedColumns" :rows="speeds" row-key="id">
          <template #cell-bar="{ row }"><ProfileMeter :value="row.value" :label="row.label" /></template>
          <template #cell-value="{ row }">{{ formatPercent(row.value) }}</template>
        </UiTable>
      </div>
    </UiSection>

    <UiSection v-if="reviewed" title="Moves per game">
      <template #note>Over {{ plural(profile.reviewed, "reviewed game") }}</template>
      <div class="pair">
        <UiTable
          v-for="(rows, n) in [classRows.slice(0, 5), classRows.slice(5)]"
          :key="n"
          label="Moves per game"
          :columns="classColumns"
          :rows="rows"
          row-key="id"
        >
          <template #cell-label="{ row }">
            <span class="cls" :class="row.tone ? `rv-${row.tone}` : 'cls--quiet'">
              <span class="cls-glyph" aria-hidden="true">{{ row.glyph }}</span>
              <span class="cls-label">{{ row.label }}</span>
            </span>
          </template>
          <template #cell-perGame="{ row }">{{ row.perGame.toFixed(2) }}</template>
        </UiTable>
      </div>
    </UiSection>

    <UiSection v-if="reviewed && profile.time.games" title="Time per move">
      <template #note>From the clocks of {{ plural(profile.time.games, "game") }}, in seconds</template>
      <UiStats dense :columns="5" class="time">
        <UiStat label="Average" :value="seconds(profile.time.average_ms)" />
        <UiStat label="Opening" :value="seconds(profile.time.opening_ms)" />
        <UiStat label="Middle" :value="seconds(profile.time.middlegame_ms)" />
        <UiStat label="Endgame" :value="seconds(profile.time.endgame_ms)" />
        <UiStat
          label="Time-trouble blunders"
          :value="profile.time.time_trouble_blunders"
          :sub="`${profile.time.time_trouble_blunders_per_game.toFixed(2)} a game`"
        />
      </UiStats>
    </UiSection>

    <UiSection v-if="moments.length" title="Best moments and worst blunders">
      <UiTable label="Best moments and worst blunders" :columns="momentColumns" :rows="moments" row-key="id">
        <template #cell-link="{ row }">
          <span class="moment" :class="row.tone ? `rv-${row.tone}` : ''">
            <span class="moment-mark" aria-hidden="true">{{ row.mark }}</span>
            <ELink :link-component="RouterLink" :to="row.to" :class="{ mono: row.mono }">{{ row.link }}</ELink>
          </span>
        </template>
        <template #cell-note="{ row }">
          <span class="muted truncate">{{ row.note }}</span>
        </template>
      </UiTable>
    </UiSection>
  </div>
</template>

<style scoped>
.profile {
  display: grid;
  gap: var(--sp-5);
  min-width: 0;
}

/* Two small tables side by side where the sheet has the room, one above the other where it does not. */
.pair {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(17rem, 1fr));
  gap: var(--sp-4) var(--sp-5);
  align-items: start;
}

.cls {
  display: inline-flex;
  align-items: baseline;
  gap: var(--sp-2);
}

.cls-glyph {
  inline-size: var(--sp-5);
  color: var(--rv);
  font-weight: 800;
}

.cls:not(.cls--quiet) .cls-label {
  color: var(--rv);
}

.cls--quiet .cls-label {
  color: var(--c-ink-2);
}

.moment {
  display: inline-flex;
  align-items: baseline;
  gap: var(--sp-2);
}

.moment-mark {
  inline-size: var(--sp-5);
  color: var(--rv);
  font-weight: 800;
}

.muted {
  display: block;
  max-width: 22rem;
  color: var(--c-ink-2);
  font: var(--t-meta);
}
</style>
