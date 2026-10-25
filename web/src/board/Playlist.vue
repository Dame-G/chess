<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import type { components } from "../api/schema";
import { outcome, type Outcome } from "./playlist";

type Game = components["schemas"]["GameSummary"];

const props = defineProps<{
  /** The games to list, in order (already filtered). */
  games: Game[];
  /** The `index` of the game on the board. */
  current: number;
}>();
const emit = defineEmits<{ open: [index: number] }>();

// Thousands of games: only the rows in view exist.
const ROW = 32;
const box = ref<HTMLElement>();
const top = ref(0);
const height = ref(600);
const first = computed(() => Math.max(0, Math.floor(top.value / ROW) - 8));
const last = computed(() => Math.min(props.games.length, Math.ceil((top.value + height.value) / ROW) + 8));
const rows = computed(() =>
  props.games.slice(first.value, last.value).map((g, i) => ({ g, n: first.value + i, o: outcome(g) as Outcome })),
);

function measure() {
  if (!box.value) return;
  top.value = box.value.scrollTop;
  height.value = box.value.clientHeight;
}

/** Brings the game on the board into view. */
async function reveal() {
  await nextTick();
  const at = props.games.findIndex((g) => g.index === props.current);
  const el = box.value;
  if (!el || at < 0) return;
  if (at * ROW < el.scrollTop || (at + 1) * ROW > el.scrollTop + el.clientHeight)
    el.scrollTop = Math.max(0, at * ROW - el.clientHeight / 2 + ROW / 2);
}
watch(() => [props.current, props.games.length], reveal);
onMounted(() => {
  measure();
  void reveal();
});

const word: Record<Outcome, string> = { W: "Won", D: "Drew", L: "Lost", "": "Unfinished" };
</script>

<template>
  <div ref="box" class="playlist" role="list" aria-label="Games" @scroll.passive="measure">
    <div class="sizer" :style="{ height: `${games.length * ROW}px` }">
      <div class="window" :style="{ transform: `translateY(${first * ROW}px)` }">
        <EButton
          v-for="r in rows"
          :key="r.g.index"
          v-memo="[r.g.index === current, r.o]"
          role="listitem"
          variant="quiet"
          size="sm"
          block
          :aria-current="r.g.index === current ? 'true' : undefined"
          :aria-label="`Game ${r.g.index + 1}, ${word[r.o]}`"
          class="row"
          @click="emit('open', r.g.index)"
        >
          <span class="n num">{{ r.g.index + 1 }}</span>
          <span :class="['side', r.g.white === 'dev' ? 'side--white' : 'side--black']" aria-hidden="true"></span>
          <span :class="['res', `res--${r.o || 'none'}`]">{{ r.o || "·" }}</span>
          <span class="len num">{{ Math.ceil(r.g.plies / 2) }}</span>
        </EButton>
      </div>
    </div>
  </div>
</template>

<style scoped>
.playlist {
  height: 100%;
  overflow-y: auto;
  overscroll-behavior: contain;
  contain: strict;
}

.sizer {
  position: relative;
}

.window {
  position: absolute;
  inset-inline: 0;
  top: 0;
  contain: layout style;
  will-change: transform;
}

.row {
  display: grid;
  grid-template-columns: 3rem 1.25rem 1.5rem 1fr;
  align-items: center;
  height: 32px;
  min-height: 0;
  padding-block: 0;
  border-radius: 0;
  font-family: var(--e-font-mono);
  text-align: start;
}

.row[aria-current] {
  background: var(--e-primary-soft);
  box-shadow: inset 3px 0 0 var(--e-primary);
  color: var(--e-ink);
  font-weight: 600;
}

.row[aria-current] .n,
.row[aria-current] .len {
  color: var(--e-ink);
}

.row:focus-visible {
  outline: var(--e-stroke-focus) solid var(--e-focus-ring);
  outline-offset: -2px;
}

.n {
  color: var(--e-muted);
  font-size: var(--e-text-sm);
  text-align: end;
  padding-inline-end: var(--e-gap-snug);
}

.side {
  width: 0.625rem;
  height: 0.625rem;
  border-radius: 50%;
  box-shadow: 0 0 0 1px var(--e-border-strong);
}

.side--white {
  background: #fffaf3;
}

.side--black {
  background: #2a2420;
}

.res {
  font-weight: 700;
  text-align: center;
}

.res--W {
  color: var(--e-success);
}

.res--L {
  color: var(--e-danger);
}

.res--D,
.res--none {
  color: var(--e-muted);
}

.len {
  color: var(--e-muted);
  font-size: var(--e-text-sm);
  text-align: end;
}
</style>
