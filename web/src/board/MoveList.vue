<script setup lang="ts">
import { computed, ref } from "vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { followCurrent } from "./follow";

const props = defineProps<{
  /** The moves in SAN, in order. */
  sans: string[];
  /** How many moves are on the board. */
  cursor: number;
  /** The FEN the line starts from, or `startpos`: it sets the first move number and who moves first. */
  start: string;
}>();
const emit = defineEmits<{ go: [ply: number] }>();

const rows = computed(() => {
  const parts = props.start.split(" ");
  const blackFirst = props.start !== "startpos" && parts[1] === "b";
  const first = props.start === "startpos" ? 1 : Number(parts[5] ?? 1);
  const all: ({ san: string; ply: number } | null)[] = [
    ...(blackFirst ? [null] : []),
    ...props.sans.map((san, i) => ({ san, ply: i + 1 })),
  ];
  const out: { number: number; plies: (typeof all)[number][] }[] = [];
  for (let i = 0; i < all.length; i += 2)
    out.push({ number: first + i / 2, plies: [all[i] ?? null, all[i + 1] ?? null] });
  return out;
});

const list = ref<HTMLElement>();
followCurrent(list, () => props.cursor);
</script>

<template>
  <div ref="list" class="moves" role="list" aria-label="Moves">
    <template v-for="r in rows" :key="r.number">
      <span class="no num" role="presentation">{{ r.number }}</span>
      <template v-for="(p, i) in r.plies" :key="i">
        <EButton
          v-if="p"
          role="listitem"
          size="sm"
          variant="quiet"
          :aria-current="p.ply === cursor ? 'step' : undefined"
          :class="{ future: p.ply > cursor }"
          @click="emit('go', p.ply)"
        >
          {{ p.san }}
        </EButton>
        <span v-else class="gap">…</span>
      </template>
    </template>
    <p v-if="!sans.length" class="none">No moves yet. Move a piece, or paste a game.</p>
  </div>
</template>

<style scoped>
.moves {
  display: grid;
  grid-template-columns: 3rem 1fr 1fr;
  align-content: start;
  height: 100%;
  overflow-y: auto;
  overscroll-behavior: contain;
  font-family: var(--e-font-mono);
}

.no {
  display: grid;
  align-items: center;
  justify-content: end;
  padding-inline: var(--e-gap-base);
  background: color-mix(in oklab, var(--e-ink) 4%, transparent);
  color: var(--e-muted);
  font: var(--e-type-small);
}

.moves :deep(.e-btn) {
  justify-content: flex-start;
  min-height: 2.25rem;
  padding-inline: var(--e-pad-control-sm);
  border-radius: 0;
  text-decoration: none;
}

/* The move on the board: the place tint, as eos marks where you are. */
.moves :deep(.e-btn[aria-current]) {
  background: var(--e-primary);
  color: var(--e-on-primary);
  font-weight: 700;
}

.moves :deep(.e-btn:focus-visible) {
  outline: var(--e-stroke-focus) solid var(--e-focus-ring);
  outline-offset: -2px;
}

.future {
  opacity: 0.72;
}

.gap {
  display: grid;
  align-items: center;
  padding-inline: var(--e-pad-control-sm);
  color: var(--e-muted);
}

.none {
  grid-column: 1 / -1;
  margin: 0;
  padding: var(--ws-pad);
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
