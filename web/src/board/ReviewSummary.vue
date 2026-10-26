<script setup lang="ts">
import { EButton } from "@eos-ui/vue/components/EButton";
import ReviewGlyph from "./ReviewGlyph.vue";
import { markOf, SUMMARY_MARKS, type Classification, type ReviewSide } from "./review";
import type { Side } from "./matchText";

/**
 * Accuracy and the counts of notable moves for each side, as Lichess's analysis summary has them. A count is a button:
 * it jumps to the next such move of that side.
 */
const props = defineProps<{
  white: ReviewSide;
  black: ReviewSide;
  names: Record<Side, string>;
}>();
const emit = defineEmits<{ jump: [side: Side, tone: Classification] }>();

const sides = ["white", "black"] as const;
const acc = (s: ReviewSide) => (s.accuracy == null ? "–" : s.accuracy.toFixed(s.accuracy >= 99.95 ? 0 : 1));
/** A miss is shown only when there is one; the other five are always there, so the two columns line up. */
const shownMarks = (s: ReviewSide) => SUMMARY_MARKS.filter((c) => c !== "miss" || s.counts.miss > 0);
const side = (s: Side) => props[s];
</script>

<template>
  <section class="sum" aria-label="Review summary">
    <div v-for="s in sides" :key="s" class="col">
      <p class="who">
        <span :class="['chip', `chip--${s}`]" aria-hidden="true"></span>
        <span class="name">{{ names[s] }}</span>
      </p>
      <p class="acc">
        <span class="big num">{{ acc(side(s)) }}<small v-if="side(s).accuracy != null">%</small></span>
        <span class="lab">accuracy</span>
        <span v-if="side(s).acpl != null" class="acpl num">{{ side(s).acpl }} ACPL</span>
      </p>
      <div class="counts" role="group" :aria-label="`${names[s]}: moves by judgement`">
        <template v-for="c in shownMarks(side(s))" :key="c">
          <EButton
            v-if="side(s).counts[c] > 0"
            variant="quiet"
            size="sm"
            :class="['count', `rv-${c}`]"
            :aria-label="`${markOf(c).label}: ${side(s).counts[c]}. Go to the next`"
            :title="markOf(c).label"
            @click="emit('jump', s, c)"
          >
            <ReviewGlyph :of="c" class="g" /><span class="n num">{{ side(s).counts[c] }}</span>
          </EButton>
          <span v-else :class="['count', 'count--none', `rv-${c}`]" :title="markOf(c).label">
            <ReviewGlyph :of="c" class="g" /><span class="n num">0</span>
          </span>
        </template>
      </div>
    </div>
  </section>
</template>

<style scoped>
.sum {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--e-gap-base);
  padding: var(--e-gap-snug) var(--ws-pad);
}

.col {
  display: grid;
  gap: var(--e-gap-tight);
  align-content: start;
  min-width: 0;
}

.col p {
  margin: 0;
}

.who {
  display: flex;
  align-items: center;
  gap: var(--e-gap-snug);
  min-width: 0;
  font: var(--e-type-small);
  font-weight: 600;
}

.name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.chip {
  flex: none;
  width: 0.625rem;
  height: 0.625rem;
  border-radius: 50%;
  box-shadow: 0 0 0 1px var(--e-border-strong);
}

.chip--white {
  background: #fffaf3;
}

.chip--black {
  background: #2a2420;
}

.acc {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 0 var(--e-gap-snug);
}

.big {
  font: 600 var(--e-text-2xl) / 1.1 var(--e-font-mono);
  letter-spacing: -0.03em;
}

.big small {
  margin-inline-start: 0.0625rem;
  color: var(--e-muted);
  font-size: 0.55em;
  font-weight: 500;
  letter-spacing: 0;
}

.lab,
.acpl {
  color: var(--e-muted);
  font: var(--e-type-small);
}

.counts {
  display: flex;
  flex-wrap: wrap;
  gap: 0;
  margin-inline: calc(-1 * var(--e-gap-tight));
}

.count {
  min-width: 0;
  padding-inline: var(--e-gap-tight);
  font-family: var(--e-font-mono);
}

.count :deep(.gl) {
  margin: 0 0.125rem 0 0;
}

.n {
  color: var(--e-ink);
  font-weight: 600;
}

.count--none {
  display: inline-flex;
  align-items: center;
  min-block-size: 2rem;
  opacity: 0.55;
}

.count--none .n {
  color: var(--e-muted);
  font-weight: 400;
}
</style>
