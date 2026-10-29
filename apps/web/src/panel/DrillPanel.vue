<script setup lang="ts">
import { computed, watch } from "vue";
import { RouterLink, useRouter } from "vue-router";
import { PhCheckCircle, PhXCircle } from "@phosphor-icons/vue";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { comesBack, prompt, sittingText } from "../board/drill";
import { sanLine } from "../board/position";
import { useBoard } from "../board/board";
import { stored } from "../stored";
import { useDrills } from "../stores/drills";
import UiToolbar from "../ui/UiToolbar.vue";

/**
 * Drill: find the move you missed. One try on the board, with no engine and no eval; then the answer, the line, the
 * game it came from, and the next drill. Nothing of the answer is shown before the try. The board shows the position;
 * the panel is the question, the progress through today's set, and a card for the answer.
 */
const drills = useDrills();
const board = useBoard();
const router = useRouter();
const toast = useToast();

const q = computed(() => drills.question);
const r = computed(() => drills.result);
const line = computed(() => (q.value && r.value ? sanLine(q.value.fen, r.value.line.slice(0, 10)) : ""));
const done = computed(() => r.value !== null && !hasNext());
const passed = computed(() => r.value?.outcome === "pass");

/** How far through the set: drills tried, out of all of them. */
const sitting = computed(() => drills.progress);
const bar = computed(() => (sitting.value.total ? (sitting.value.tried / sitting.value.total) * 100 : 0));
const where = computed(() => sittingText(sitting.value, r.value !== null));

/** The instruction is said until a drill has been tried once; after that the board explains itself. */
const learned = stored("drill.learned", false);
watch(r, (v) => {
  if (v) learned.value = true;
});

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
/** Leaves the set and looks at the drill's position in Explore. */
function explore() {
  const fen = q.value?.fen;
  if (!fen) return;
  drills.leave();
  board.openFree(fen);
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
    <div
      class="bar"
      role="progressbar"
      :aria-valuenow="sitting.tried"
      :aria-valuemax="sitting.total"
      :aria-valuetext="where"
      aria-label="Today's drills"
    >
      <span class="bar-fill" :style="{ inlineSize: `${bar}%` }" />
    </div>

    <header class="ask">
      <h2 class="prompt">{{ prompt(q) }}</h2>
      <p v-if="q.opening || q.phase" class="meta">
        <template v-if="q.opening">{{ q.opening }}</template>
        <template v-if="q.opening && q.phase"> · </template>
        <template v-if="q.phase">{{ q.phase }}</template>
      </p>
    </header>

    <template v-if="!r">
      <p v-if="!learned" class="meta">One try: make the move on the board.</p>
      <UiToolbar align="between" label="Drill">
        <EButton variant="outline" @click="skip">Skip</EButton>
        <template #end><EButton variant="quiet" @click="leave">Stop</EButton></template>
      </UiToolbar>
    </template>
    <template v-else>
      <section class="card" :class="passed ? 'card--pass' : 'card--fail'" aria-label="The answer" aria-live="polite">
        <p class="verdict">
          <component :is="passed ? PhCheckCircle : PhXCircle" class="verdict-icon" aria-hidden="true" />
          {{ passed ? "Correct" : "Not this one" }}
        </p>
        <p class="detail">
          You played <strong class="san">{{ r.played_san }}</strong
          >.
          <template v-if="!passed">
            Best was <strong class="san">{{ r.best_san.join(" or ") }}</strong
            >.
          </template>
        </p>
        <p class="line num" aria-label="The line">{{ line }}</p>
        <p class="meta">
          It comes back {{ comesBack(r.drill.interval_days) }}.
          <RouterLink class="ui-link" :to="{ path: '/', query: { game: String(q.game), ply: String(q.ply) } }">
            The game it came from, move {{ Math.ceil(q.ply / 2) }}
          </RouterLink>
        </p>
        <p v-if="done" class="meta">{{ where }}</p>
      </section>
      <UiToolbar align="between" label="Drill">
        <EButton variant="primary" @click="next">{{ done ? "Finish" : "Next" }}</EButton>
        <EButton variant="outline" @click="explore">Explore this position</EButton>
        <template v-if="!done" #end><EButton variant="quiet" @click="leave">Stop</EButton></template>
      </UiToolbar>
    </template>
  </div>
</template>

<style scoped>
.drill {
  display: grid;
  gap: var(--sp-3);
  min-width: 0;
}

/* A thin bar over the panel: how far through today's set. */
.bar {
  block-size: var(--e-stroke-focus);
  margin-inline: calc(-1 * var(--sp-3));
  margin-block-start: calc(-1 * var(--sp-3));
  background: var(--c-sunk);
}

.bar-fill {
  display: block;
  block-size: 100%;
  background: var(--c-act);
  transition: inline-size var(--m-move) var(--m-ease);
}

@media (prefers-reduced-motion: reduce) {
  .bar-fill {
    transition: none;
  }
}

.ask {
  display: grid;
  gap: var(--sp-1);
}

.prompt {
  margin: 0;
  font: var(--t-head);
}

.meta {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.meta::first-letter {
  text-transform: uppercase;
}

/* The answer: the verdict first, in the outcome's colour, then what was played and what was best. */
.card {
  display: grid;
  gap: var(--sp-2);
  padding: var(--sp-3);
  border-radius: var(--r-1);
  background: var(--c-raise);
  box-shadow: inset var(--e-stroke-focus) 0 0 var(--tone);
}

.card--pass {
  --tone: var(--c-win);
}

.card--fail {
  --tone: var(--c-loss);
}

.verdict {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  margin: 0;
  color: var(--tone);
  font: var(--t-head);
}

.verdict-icon {
  flex: none;
  font-size: var(--i-lg);
}

.detail {
  margin: 0;
}

.san {
  font: var(--t-fig);
  font-weight: 700;
}

.line {
  margin: 0;
  font: var(--t-fig);
  overflow-wrap: anywhere;
}
</style>
