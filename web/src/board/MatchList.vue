<script setup lang="ts">
import { EButton } from "@eos-ui/vue/components/EButton";
import { matchTimeText, type MatchSummary } from "./matchText";

defineProps<{
  matches: MatchSummary[];
  /** The match on the board. */
  current?: number;
}>();
const emit = defineEmits<{ open: [id: number] }>();

const shown = (r: MatchSummary["result"]) => (r === "1/2-1/2" ? "½–½" : r === "*" ? "·" : r.replace("-", "–"));
</script>

<template>
  <div class="matches" role="list" aria-label="Matches">
    <EButton
      v-for="m in matches"
      :key="m.id"
      role="listitem"
      variant="quiet"
      size="sm"
      block
      class="row"
      :aria-current="m.id === current ? 'true' : undefined"
      :aria-label="`${m.white.name} against ${m.black.name}${m.status === 'running' ? ', live' : ''}`"
      @click="emit('open', m.id)"
    >
      <span v-if="m.status === 'running'" class="res res--live num"
        ><span class="dot" aria-hidden="true"></span>Live</span
      >
      <span v-else class="res num">{{ shown(m.result) }}</span>
      <span class="what">
        <span class="names">
          <span class="n">{{ m.white.name }}</span>
          <span class="vs">vs</span>
          <span class="n">{{ m.black.name }}</span>
        </span>
        <span class="tc num">{{ matchTimeText(m.time) }} · {{ Math.ceil(m.plies / 2) }} moves</span>
      </span>
    </EButton>
  </div>
</template>

<style scoped>
.matches {
  display: grid;
  align-content: start;
}

.row {
  display: grid;
  grid-template-columns: 3.25rem minmax(0, 1fr);
  align-items: center;
  gap: var(--e-gap-snug);
  min-height: 2.75rem;
  padding-block: 0.25rem;
  border-radius: 0;
  text-align: start;
}

.row[aria-current] {
  background: var(--e-primary-soft);
  box-shadow: inset 3px 0 0 var(--e-primary);
  color: var(--e-ink);
  font-weight: 600;
}

.row:focus-visible {
  outline: var(--e-stroke-focus) solid var(--e-focus-ring);
  outline-offset: -2px;
}

.res {
  color: var(--e-muted);
  font-size: var(--e-text-sm);
  font-weight: 700;
  white-space: nowrap;
}

.res--live {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  color: var(--e-info, var(--e-primary));
}

.dot {
  width: 0.5rem;
  height: 0.5rem;
  border-radius: 50%;
  background: currentcolor;
  animation: pulse 1.6s ease-in-out infinite;
}

@keyframes pulse {
  50% {
    opacity: 0.3;
  }
}

@media (prefers-reduced-motion: reduce) {
  .dot {
    animation: none;
  }
}

.what {
  display: grid;
  min-width: 0;
}

.names {
  display: flex;
  gap: 0.375rem;
  align-items: baseline;
  min-width: 0;
  overflow: hidden;
  white-space: nowrap;
}

.n {
  overflow: hidden;
  text-overflow: ellipsis;
}

.vs,
.tc {
  flex: none;
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
