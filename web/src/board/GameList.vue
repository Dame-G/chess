<script setup lang="ts">
/** The Games tab: every game kept here, live ones first, to open on the board. */
import { computed, ref } from "vue";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { isMine } from "./lichessMe";
import MatchList from "./MatchList.vue";
import type { MatchSummary } from "./matchText";

const props = defineProps<{
  matches: MatchSummary[];
  loading?: boolean;
  /** The match on the board. */
  current?: number;
}>();
const emit = defineEmits<{ open: [id: number] }>();

/** Whose game it is: one you played (here or on Lichess), one of the bot's on Lichess, or two of our engines. */
const kindOf = (m: MatchSummary) =>
  isMine(m) || (!m.lichess && (m.white.human || m.black.human)) ? "mine" : m.lichess ? "bot" : "engines";
const filter = ref("all");
const options = computed(() => {
  const kinds = new Set(props.matches.map(kindOf));
  return [
    { value: "all", label: "All" },
    ...[
      { value: "mine", label: "Mine" },
      { value: "engines", label: "Engines" },
      { value: "bot", label: "Bot" },
    ].filter((o) => kinds.has(o.value as ReturnType<typeof kindOf>)),
  ];
});
const shown = computed(() => {
  const f = options.value.some((o) => o.value === filter.value) ? filter.value : "all";
  // A game cancelled before the first move leaves nothing to replay.
  const list = props.matches
    .filter((m) => m.plies > 0 || m.status !== "aborted")
    .filter((m) => f === "all" || kindOf(m) === f);
  // What is being played comes first.
  return [...list.filter((m) => m.status === "running"), ...list.filter((m) => m.status !== "running")];
});
</script>

<template>
  <div class="list">
    <div v-if="options.length > 1" class="bar">
      <ESegmented v-model="filter" label="Show games" hide-label size="sm" :options="options" />
      <span class="count num">{{ shown.length }}</span>
    </div>
    <ESkeleton v-if="loading && !matches.length" :lines="4" label="Loading games" class="pad" />
    <EEmptyState v-else-if="!shown.length" title="No games yet" icon="info" size="sm" bare class="pad">
      Games you play or watch are kept here to replay.
    </EEmptyState>
    <MatchList v-else :matches="shown" :current="current" @open="emit('open', $event)" />
  </div>
</template>

<style scoped>
.list {
  display: grid;
  align-content: start;
}

.bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--e-gap-snug);
  padding: var(--e-gap-snug) var(--ws-pad);
  border-block-end: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 40%, transparent);
}

.count {
  color: var(--e-muted);
  font: var(--e-type-small);
}

.pad {
  padding: var(--e-pad-control-sm);
}
</style>
