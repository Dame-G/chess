<script setup lang="ts">
import { computed } from "vue";
import { useRoute } from "vue-router";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import type { TrialStatus } from "../api/types";
import { signed, trialClock } from "../format";
import { stageLabel } from "../rail/rail";
import { useIdeas } from "../stores/ideas";
import SheetFrame from "./SheetFrame.vue";

/**
 * An idea: its hypothesis and notes, the funnel as a row of stages with the deciding numbers, the selected trial's
 * evidence, its games, a tune's trajectory, and Promote when kept. For now: the hypothesis, the verdict and the
 * funnel's stages.
 */
const route = useRoute();
const ideas = useIdeas();
const idea = computed(() => ideas.ideas[Number(route.params["id"])]);

const tone: Record<TrialStatus, "neutral" | "info" | "success" | "warning" | "danger"> = {
  waiting: "neutral",
  running: "info",
  passed: "success",
  failed: "danger",
  stopped: "neutral",
  skipped: "neutral",
};
</script>

<template>
  <SheetFrame :title="idea?.title ?? 'Idea'" :description="idea?.hypothesis">
    <EEmptyState v-if="!idea" title="No such idea">It may have been removed, or the link is wrong.</EEmptyState>
    <template v-else>
      <p v-if="idea.verdict" class="verdict">{{ idea.verdict }}</p>
      <ol class="funnel">
        <li v-for="t in idea.trials" :key="t.id" class="stage">
          <span class="stage-name"
            >{{ stageLabel[t.stage] }}<template v-if="t.time"> · {{ trialClock(t.time) }}</template></span
          >
          <EStamp size="sm" :tone="tone[t.status]" :label="t.status" />
          <span class="stage-elo num">{{
            t.elo ? `${signed(t.elo.elo, 1)} ± ${((t.elo.high - t.elo.low) / 2).toFixed(1)}` : ""
          }}</span>
          <p v-if="t.detail" class="stage-detail">{{ t.detail }}</p>
        </li>
      </ol>
      <p v-if="idea.notes" class="notes">{{ idea.notes }}</p>
    </template>
  </SheetFrame>
</template>

<style scoped>
.verdict {
  margin-block-start: 0;
  font-weight: 600;
}

.funnel {
  display: grid;
  margin: 0;
  padding: 0;
  list-style: none;
}

.stage {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto 7rem;
  align-items: center;
  gap: var(--e-gap-tight) var(--e-gap-snug);
  padding-block: var(--e-gap-snug);
  border-block-end: var(--line);
}

.stage-name {
  font-weight: 600;
}

.stage-name::first-letter {
  text-transform: uppercase;
}

.stage-elo {
  font-family: var(--e-font-mono);
  text-align: end;
}

.stage-detail,
.notes {
  grid-column: 1 / -1;
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.notes {
  margin-block-start: var(--e-gap-base);
}
</style>
