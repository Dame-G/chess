<script setup lang="ts">
import { computed } from "vue";
import { PhArrowSquareOut } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ago } from "../../format";
import { gameClock, opponentOf, oursOf, oursText, speedName } from "../../lichess";
import type { MatchSummary } from "../../board/matchText";

const props = defineProps<{ games: MatchSummary[] }>();
const emit = defineEmits<{ open: [id: number] }>();

// A game being played is first.
const rows = computed(() =>
  [...props.games].sort((a, b) => Number(b.status === "running") - Number(a.status === "running")),
);
</script>

<template>
  <div class="games" role="list" aria-label="Lichess games">
    <div v-for="m in rows" :key="m.id" class="item" role="listitem">
      <EButton
        variant="quiet"
        size="sm"
        block
        class="row"
        :aria-label="`${oursText[oursOf(m)]} against ${opponentOf(m).player.name}`"
        @click="emit('open', m.id)"
      >
        <span :class="['res', `res--${oursOf(m)}`]">
          <span v-if="m.status === 'running'" class="dot" aria-hidden="true"></span>{{ oursText[oursOf(m)] }}
        </span>
        <span class="what">
          <span class="opp">
            <span v-if="opponentOf(m).player.title" class="title">{{ opponentOf(m).player.title }}</span>
            <span class="name">{{ opponentOf(m).player.name }}</span>
            <span v-if="opponentOf(m).player.rating" class="rating num">{{ opponentOf(m).player.rating }}</span>
          </span>
          <span class="meta num">
            As {{ opponentOf(m).side === "white" ? "white" : "black" }} · {{ speedName(m.lichess!.speed) }}
            {{ gameClock(m) }} · {{ m.lichess!.rated ? "rated" : "casual" }} · {{ ago(m.created_at) }}
          </span>
        </span>
      </EButton>
      <EButton
        variant="quiet"
        size="sm"
        :icon="PhArrowSquareOut"
        :href="m.lichess!.url"
        external
        aria-label="Open on Lichess"
        title="Open on Lichess"
      />
    </div>
  </div>
</template>

<style scoped>
.games {
  display: grid;
  align-content: start;
}

.item {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: center;
  padding-inline-end: var(--e-gap-snug);
}

.row {
  display: grid;
  grid-template-columns: 3rem minmax(0, 1fr);
  align-items: baseline;
  gap: var(--e-gap-snug);
  /* A row holds two lines, so it grows with them: the button's own fixed control height would let the text spill over the next row. */
  block-size: auto;
  min-block-size: 2.75rem;
  padding: var(--e-gap-snug) var(--ws-pad);
  border-radius: 0;
  text-align: start;
  text-decoration: none;
}

.res {
  display: inline-flex;
  align-items: center;
  line-height: 1.25rem;
  gap: 0.25rem;
  color: var(--e-muted);
  font-size: var(--e-text-sm);
  font-weight: 700;
}

.res--won {
  color: var(--e-success);
}

.res--lost {
  color: var(--e-danger);
}

.res--live {
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

.opp {
  display: flex;
  line-height: 1.25rem;
  align-items: baseline;
  gap: 0.375rem;
  min-width: 0;
}

.name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.title {
  color: var(--e-primary);
  font: var(--e-type-small);
  font-weight: 700;
}

.rating,
.meta {
  color: var(--e-muted);
  font: var(--e-type-small);
}

.meta {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
