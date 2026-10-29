<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { PhListNumbers } from "@phosphor-icons/vue";
import Glyph from "../board/Glyph.vue";
import type { StripMove } from "./strip";

/**
 * On a phone the moves are one row under the board: `1. e4 e5 2. Nf3 …`, the move on the board kept in the middle, and
 * a Moves button that opens the whole list.
 */

const props = defineProps<{ moves: StripMove[] }>();
const emit = defineEmits<{ go: [id: number]; open: [] }>();

const row = ref<HTMLElement>();
const reduced = typeof matchMedia === "function" ? matchMedia("(prefers-reduced-motion: reduce)") : undefined;

/** Keeps the move on the board in the middle of the row. */
function centre() {
  const el = row.value?.querySelector<HTMLElement>("[aria-current]");
  if (!el || !row.value) return;
  const box = row.value;
  const to = el.offsetLeft - (box.clientWidth - el.offsetWidth) / 2;
  box.scrollTo({ left: to, behavior: reduced?.matches ? "instant" : "smooth" });
}

function onClick(e: MouseEvent) {
  const id = (e.target as Element).closest<HTMLElement>("[data-id]")?.dataset["id"];
  if (id !== undefined) emit("go", Number(id));
}

watch(
  () => props.moves.find((m) => m.current)?.id,
  () => void nextTick(centre),
  { immediate: true, flush: "post" },
);
</script>

<template>
  <div class="strip">
    <!-- eslint-disable-next-line vuejs-accessibility/no-static-element-interactions, vuejs-accessibility/click-events-have-key-events -- the moves inside are buttons: the click is delegated, the keys are theirs -->
    <div ref="row" class="line" role="group" aria-label="Moves" @click="onClick">
      <template v-for="m in moves" :key="m.id">
        <span v-if="m.number" class="n num">{{ m.number }}</span>
        <button
          type="button"
          class="m"
          :data-id="m.id"
          :aria-current="m.current ? 'true' : undefined"
          :tabindex="m.current ? 0 : -1"
        >
          {{ m.san }}<Glyph :of="m.mark" />
        </button>
      </template>
      <span v-if="!moves.length" class="none">No moves yet</span>
    </div>
    <button type="button" class="all" aria-label="All moves" @click="emit('open')">
      <PhListNumbers aria-hidden="true" /> Moves
    </button>
  </div>
</template>

<style scoped>
.strip {
  display: flex;
  align-items: stretch;
  min-width: 0;
  border-block: var(--e-stroke-hair) solid var(--c-line);
  background: var(--c-pane);
}

.line {
  display: flex;
  flex: 1 1 auto;
  align-items: center;
  min-width: 0;
  padding-inline: var(--sp-2);
  overflow-x: auto;
  scrollbar-width: none;
  white-space: nowrap;
}

.n {
  padding-inline: var(--sp-1) 0;
  color: var(--c-ink-2);
  font: var(--t-fig);
  font-size: var(--e-text-xs);
}

.m {
  flex: none;
  min-height: var(--h-move);
  margin: 0;
  padding: 0 var(--sp-2);
  border: 0;
  border-radius: var(--r-1);
  background: transparent;
  color: var(--c-ink);
  font: var(--t-fig);
  cursor: pointer;
}

.m[aria-current] {
  background: var(--c-sunk);
  box-shadow: inset 0 calc(-1 * var(--e-stroke-focus)) 0 var(--c-act);
  font-weight: 700;
}

.m:focus-visible,
.all:focus-visible {
  outline: var(--focus-ring);
  outline-offset: calc(-1 * var(--e-stroke-focus));
}

.none {
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.all {
  display: inline-flex;
  flex: none;
  align-items: center;
  gap: var(--sp-1);
  min-height: var(--h-control);
  margin: 0;
  padding: 0 var(--sp-3);
  border: 0;
  border-inline-start: var(--e-stroke-hair) solid var(--c-line);
  background: var(--c-pane);
  color: var(--c-ink);
  font: var(--t-body-strong);
  cursor: pointer;
}
</style>
