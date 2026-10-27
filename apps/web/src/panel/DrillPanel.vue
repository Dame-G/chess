<script setup lang="ts">
import { computed } from "vue";
import { RouterLink, useRouter } from "vue-router";
import { useToast } from "@eos-ui/vue";
import { EActions } from "@eos-ui/vue/components/EActions";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ELink } from "@eos-ui/vue/components/ELink";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import { prompt, sittingText, verdict } from "../board/drill";
import { sanLine } from "../board/position";
import { useBoard } from "../board/board";
import { useDrills } from "../stores/drills";

/**
 * Drill: find the move you missed. One try on the board, with no engine and no eval; then the answer, the line, the
 * game it came from, and the next drill. Nothing of the answer is shown before the try.
 */
const drills = useDrills();
const board = useBoard();
const router = useRouter();
const toast = useToast();

const q = computed(() => drills.question);
const r = computed(() => drills.result);
const line = computed(() => (q.value && r.value ? sanLine(q.value.fen, r.value.line.slice(0, 10)) : ""));
const done = computed(() => r.value !== null && !hasNext());

function hasNext() {
  return drills.queue.some((d) => d.id !== drills.active?.id && !drills.outcomes.has(d.id));
}

function next() {
  if (!drills.next()) leave();
}
function leave() {
  drills.leave();
  void router.push({ path: "/", query: board.address });
}
async function skip() {
  try {
    if (!drills.next()) leave();
  } catch (e) {
    toast.show({
      title: "That did not go through",
      message: e instanceof Error ? e.message : String(e),
      tone: "warning",
    });
  }
}
</script>

<template>
  <div v-if="q" class="drill">
    <p class="ask">{{ prompt(q) }}</p>
    <p class="meta num">
      {{ sittingText(drills.progress, r !== null) }}
      <template v-if="q.opening"> · {{ q.opening }}</template>
      <template v-if="q.phase"> · {{ q.phase }}</template>
    </p>

    <template v-if="!r">
      <p class="muted">One try: make the move on the board.</p>
      <EActions>
        <EButton size="sm" variant="outline" @click="skip">Skip</EButton>
      </EActions>
    </template>
    <template v-else>
      <p class="result">
        <EStamp
          size="sm"
          :tone="r.outcome === 'pass' ? 'success' : 'danger'"
          :label="r.outcome === 'pass' ? 'Pass' : 'Fail'"
        />
        <span>{{ verdict(r) }}</span>
      </p>
      <p class="line num" aria-label="The line">{{ line }}</p>
      <p class="muted">
        <ELink :link-component="RouterLink" :to="{ path: '/', query: { game: String(q.game), ply: String(q.ply) } }">
          The game it came from, move {{ Math.ceil(q.ply / 2) }}
        </ELink>
      </p>
      <EActions>
        <EButton variant="secondary" @click="next">{{ done ? "Finish" : "Next" }}</EButton>
        <EButton v-if="!done" size="sm" variant="outline" @click="leave">Stop</EButton>
      </EActions>
    </template>
  </div>
</template>

<style scoped>
.drill {
  display: grid;
  gap: var(--e-gap-snug);
  min-width: 0;
}

.ask {
  margin: 0;
  font-weight: 700;
}

.meta,
.muted {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.result {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--e-gap-snug);
  margin: 0;
}

.line {
  margin: 0;
  font: 0.8125rem/1.5 var(--e-font-mono);
}
</style>
