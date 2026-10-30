<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { RouterLink, useRouter } from "vue-router";
import { PhCheckCircle, PhLightbulb, PhMinusCircle, PhXCircle } from "@phosphor-icons/vue";
import { formatShortcut, useShortcuts, useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EKbd } from "@eos-ui/vue/components/EKbd";
import { comesBack, prompt, scopeTitle, sittingText } from "../board/drill";
import { sanLine } from "../board/position";
import { THEMES, themeText } from "../board/themes";
import { useBoard } from "../board/board";
import { plural } from "../format";
import { stored } from "../stored";
import { useDrills } from "../stores/drills";
import UiBadge from "../ui/UiBadge.vue";
import UiToolbar from "../ui/UiToolbar.vue";
import DrillSummary from "./DrillSummary.vue";

/**
 * Drill: find the move you missed. One try on the board, with no engine and no eval; then the answer, the line, what
 * the mistake was about, and the next drill. Nothing of the answer is shown before the try, not even the theme: the
 * hint is the only help, and it counts against the answer. The board shows the position; the panel is the question,
 * the progress through the sitting, and a card for the answer. The sitting ends in its summary.
 *
 * Keys: H hint, S skip, Enter or → on to the next drill, E explore the position, Esc ends the sitting.
 */
const drills = useDrills();
const board = useBoard();
const router = useRouter();
const toast = useToast();

const q = computed(() => drills.question);
const r = computed(() => drills.result);
const line = computed(() => (q.value && r.value ? sanLine(q.value.fen, r.value.line.slice(0, 10)) : ""));
const passed = computed(() => r.value?.outcome === "pass");
const halved = computed(() => passed.value && r.value?.hinted === true);
const last = computed(() => !drills.queue.some((d) => d.id !== drills.active?.id && !drills.outcomes.has(d.id)));

/** How far through the sitting: drills tried, out of all of them. */
const sitting = computed(() => drills.progress);
const where = computed(() => sittingText(sitting.value, r.value !== null));
const title = computed(() => scopeTitle(drills.scope));
const label = computed(() => `${title.value}: ${where.value}`);
const pipLabel = { pass: "right", half: "right with a hint", fail: "missed", now: "now", todo: "to come" } as const;

const verdict = computed(() => (halved.value ? "Right, with a hint" : passed.value ? "Correct" : "Not this one"));
const tone = computed(() => (halved.value ? "half" : passed.value ? "pass" : "fail"));
const theme = computed(() => {
  const t = r.value?.drill.theme;
  return t && t.kind !== "other" ? { text: themeText(t.kind, t.detail), evidence: THEMES[t.kind].evidence } : null;
});
const canHint = computed(() => !r.value && !drills.answering && drills.hints.length < 2);

/** The instruction is said until a drill has been tried once; after that the board explains itself. */
const learned = stored("drill.learned", false);
/** The Next button takes focus when the answer comes, so Enter goes on whether or not a key was caught. */
const nextButton = ref<{ $el?: HTMLElement } | null>(null);
const hintsEl = ref<HTMLElement | null>(null);
const cardEl = ref<HTMLElement | null>(null);
/** On a phone the page scrolls and the controls hold its foot: what they just produced is brought up over them. */
const reveal = (el: HTMLElement | null) => el?.scrollIntoView?.({ block: "nearest", behavior: "smooth" });
watch(r, async (v) => {
  if (!v) return;
  learned.value = true;
  await nextTick();
  nextButton.value?.$el?.focus?.({ preventScroll: true });
  reveal(cardEl.value);
});

async function hint() {
  if (!canHint.value) return;
  try {
    await drills.hint();
    await nextTick();
    reveal(hintsEl.value);
  } catch (e) {
    toast.show({
      title: "No hint",
      message: e instanceof Error ? e.message : String(e),
      tone: "warning",
    });
  }
}
/** On to the next drill, or the summary after the last. */
function next() {
  if (!r.value) return;
  if (!drills.next()) drills.finish();
}
/** Ends the sitting where it stands: its summary when something was answered, else back to the board. */
function stop() {
  drills.finish();
  if (!drills.finished) leave();
}
function leave() {
  drills.leave();
  void router.push({ path: "/", query: board.address });
}
/** Leaves the sitting and looks at the drill's position in Explore. */
function explore() {
  const fen = q.value?.fen;
  if (!fen) return;
  drills.leave();
  board.openFree(fen);
}
function skip() {
  if (r.value) return;
  try {
    if (!drills.next()) stop();
  } catch (e) {
    toast.show({
      title: "That did not go through",
      message: e instanceof Error ? e.message : String(e),
      tone: "warning",
    });
  }
}

const onButton = (e: KeyboardEvent) => !!(e.target as Element | null)?.closest?.("button, a, [role=button]");
const dialogOpen = () => !!document.querySelector("[role=dialog], [role=alertdialog]");
useShortcuts(
  {
    enter: (e) => {
      // A focused button takes Enter itself; anywhere else it goes on.
      if (!onButton(e)) next();
    },
    right: () => next(),
    h: () => void hint(),
    s: skip,
    e: () => r.value && explore(),
    escape: () => {
      if (!dialogOpen() && !drills.finished) stop();
    },
  },
  { enabled: () => q.value !== null && !drills.finished, preventDefault: false },
);
const keys = { hint: formatShortcut("h"), next: formatShortcut("enter") };
</script>

<template>
  <DrillSummary v-if="drills.finished" />
  <div v-else-if="q" class="drill">
    <div class="head">
      <p class="where" role="status">{{ label }}</p>
      <p v-if="drills.run >= 2" class="streak">
        <span class="streak-n num">{{ drills.run }}</span> in a row
      </p>
    </div>
    <ol class="pips" :aria-label="`Progress: ${where}`">
      <li v-for="(p, i) in drills.strip" :key="i" class="pip" :class="`pip--${p}`" :title="`${i + 1}: ${pipLabel[p]}`">
        <span class="sr">{{ pipLabel[p] }}</span>
      </li>
    </ol>

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
      <ol v-if="drills.hints.length" ref="hintsEl" class="hints" aria-label="Hints" aria-live="polite">
        <li v-for="h in drills.hints" :key="h.level" class="hint">
          <PhLightbulb class="hint-icon" aria-hidden="true" />{{ h.text }}
        </li>
      </ol>
      <UiToolbar class="controls" align="between" label="Drill">
        <EButton
          variant="outline"
          :disabled="!canHint"
          :loading="drills.hinting"
          aria-keyshortcuts="H"
          :title="drills.hints.length >= 2 ? 'No more hints' : 'A right answer with a hint counts half'"
          @click="hint"
        >
          {{ drills.hints.length ? "Another hint" : "Hint" }}
          <EKbd v-if="canHint" size="sm" class="kbd">{{ keys.hint.join("") }}</EKbd>
        </EButton>
        <EButton variant="quiet" aria-keyshortcuts="S" @click="skip">Skip</EButton>
        <template #end><EButton variant="quiet" @click="stop">Stop</EButton></template>
      </UiToolbar>
      <p v-if="drills.hints.length" class="meta">A right answer with a hint counts half.</p>
    </template>
    <template v-else>
      <section ref="cardEl" class="card" :class="`card--${tone}`" aria-label="The answer" aria-live="polite">
        <p class="verdict">
          <component
            :is="halved ? PhMinusCircle : passed ? PhCheckCircle : PhXCircle"
            class="verdict-icon"
            aria-hidden="true"
          />
          {{ verdict }}
        </p>
        <p class="detail">
          You played <strong class="san">{{ r.played_san }}</strong
          >.
          <template v-if="!passed">
            Best was <strong class="san">{{ r.best_san.join(" or ") }}</strong
            >.
          </template>
          <template v-else-if="r.best_san.length > 1">
            Also good: <strong class="san">{{ r.best_san.join(", ") }}</strong
            >.
          </template>
        </p>
        <p class="line num" aria-label="The line">{{ line }}</p>
        <p v-if="theme" class="theme">
          <UiBadge>{{ theme.text }}</UiBadge>
          <span v-if="!passed || halved" class="meta">{{ theme.evidence }}</span>
        </p>
        <p class="meta">
          It comes back {{ comesBack(r.drill.interval_days) }}.
          <RouterLink class="ui-link" :to="{ path: '/', query: { game: String(q.game), ply: String(q.ply) } }">
            The game it came from, move {{ Math.ceil(q.ply / 2) }}
          </RouterLink>
        </p>
      </section>
      <UiToolbar class="controls" align="between" label="Drill">
        <EButton ref="nextButton" variant="primary" aria-keyshortcuts="Enter" @click="next">
          {{ last ? "Finish" : "Next" }}
          <EKbd size="sm" class="kbd">{{ keys.next.join("") }}</EKbd>
        </EButton>
        <EButton variant="outline" aria-keyshortcuts="E" @click="explore">Explore this position</EButton>
        <template v-if="!last" #end><EButton variant="quiet" @click="stop">Stop</EButton></template>
      </UiToolbar>
      <p v-if="last" class="meta">{{ plural(sitting.total, "drill") }} done.</p>
    </template>
  </div>
</template>

<style scoped>
.drill {
  display: grid;
  gap: var(--sp-3);
  min-width: 0;
}

.head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--sp-3);
  margin-block-end: calc(-1 * var(--sp-2));
}

.where,
.streak {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.streak-n {
  color: var(--c-ink);
  font: var(--t-fig);
  font-weight: 700;
}

/* One segment for each drill of the sitting: what came of it, the one on the board, and those still to come. */
.pips {
  display: flex;
  gap: var(--sp-1);
  margin: 0;
  padding: 0;
  list-style: none;
}

.pip {
  flex: 1 1 0;
  max-inline-size: var(--sp-6);
  block-size: var(--sp-1);
  border-radius: var(--r-1);
  background: var(--c-sunk);
  transition: background var(--m-move) var(--m-ease);
}

.pip--pass {
  background: var(--c-win);
}

.pip--half {
  background: var(--c-warn);
}

.pip--fail {
  background: var(--c-loss);
}

.pip--now {
  background: var(--c-act);
}

@media (prefers-reduced-motion: reduce) {
  .pip {
    transition: none;
  }
}

.sr {
  position: absolute;
  inline-size: 1px;
  block-size: 1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
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

.kbd {
  margin-inline-start: var(--sp-1);
}

@media (pointer: coarse) {
  .kbd {
    display: none;
  }
}

/* The hints asked for: what kind of move, then which piece. */
.hints {
  display: grid;
  gap: var(--sp-1);
  margin: 0;
  padding: var(--sp-2) var(--sp-3);
  border-radius: var(--r-1);
  background: var(--c-raise);
  list-style: none;
}

.hint {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
}

.hint-icon {
  flex: none;
  color: var(--c-warn);
  font-size: var(--i-md);
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

.card--half {
  --tone: var(--c-warn);
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

.theme {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--sp-1) var(--sp-2);
  margin: 0;
}

/* On a phone the page scrolls: the controls hold the foot of the screen, over the tab bar, so Hint, Skip and Next are
   in reach whatever the hints and the answer push down. */
@media (width < 48rem) {
  .hints,
  .card {
    scroll-margin-block-end: calc(var(--h-tabbar) + var(--h-control) + var(--sp-4));
  }

  .controls {
    position: sticky;
    inset-block-end: calc(var(--h-tabbar) + env(safe-area-inset-bottom, 0px));
    z-index: var(--e-z-sticky);
    padding-block: var(--sp-2);
    background: var(--c-pane);
    box-shadow: 0 calc(-1 * var(--e-stroke-hair)) 0 var(--c-line);
  }
}
</style>
