<script setup lang="ts">
import { computed, ref } from "vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { followCurrent } from "./follow";
import { rows, type GameTree, type TreeNode } from "./tree";

/**
 * Explore's move list: the main line in numbered pairs and, under a move with alternatives, its variations as runs of
 * moves (deeper ones indented), as Lichess shows a study. Any move is a click away.
 */
const props = defineProps<{
  tree: GameTree;
  /** The node on the board. */
  node: TreeNode;
  /** Bumped when the tree changes in place, so the rows are read again. */
  version: number;
}>();
const emit = defineEmits<{ select: [node: TreeNode] }>();

const list = computed(() => {
  void props.version;
  return rows(props.tree);
});
const box = ref<HTMLElement>();
followCurrent(box, () => props.node.id);
</script>

<template>
  <div ref="box" class="tree" role="group" aria-label="Moves and variations">
    <template
      v-for="row in list"
      :key="row.type === 'moves' ? `m${row.number}-${row.white?.id}-${row.black?.id}` : row.key"
    >
      <template v-if="row.type === 'moves'">
        <span class="no num" role="presentation">{{ row.number }}</span>
        <EButton
          v-if="row.white"
          size="sm"
          variant="quiet"
          class="mv"
          :aria-current="row.white.id === node.id ? 'step' : undefined"
          @click="emit('select', row.white)"
        >
          {{ row.white.san }}
        </EButton>
        <span v-else class="gap">…</span>
        <EButton
          v-if="row.black"
          size="sm"
          variant="quiet"
          class="mv"
          :aria-current="row.black.id === node.id ? 'step' : undefined"
          @click="emit('select', row.black)"
        >
          {{ row.black.san }}
        </EButton>
        <span v-else class="gap" aria-hidden="true"></span>
      </template>
      <div v-else class="vars">
        <p v-for="(line, i) in row.lines" :key="i" class="var">
          <EButton
            v-for="t in line"
            :key="t.node.id"
            size="sm"
            variant="quiet"
            class="vm"
            :class="{ deep: t.depth > 1 }"
            :aria-current="t.node.id === node.id ? 'step' : undefined"
            @click="emit('select', t.node)"
          >
            {{ t.text }}
          </EButton>
        </p>
      </div>
    </template>
    <p v-if="!list.length" class="none">No moves yet. Move a piece, or paste a FEN or PGN.</p>
  </div>
</template>

<style scoped>
.tree {
  display: grid;
  grid-template-columns: 2.5rem 1fr 1fr;
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
  padding-inline: var(--e-gap-snug);
  background: color-mix(in oklab, var(--e-ink) 4%, transparent);
  color: var(--e-muted);
  font: var(--e-type-small);
}

.mv,
.vm {
  justify-content: flex-start;
  height: 2rem;
  padding-inline: var(--ws-pad, var(--e-pad-control));
  border-radius: 0;
  font-family: var(--e-font-mono);
  text-decoration: none;
}

.mv[aria-current],
.vm[aria-current] {
  background: var(--e-primary);
  color: var(--e-on-primary);
  font-weight: 700;
}

.gap {
  display: grid;
  align-items: center;
  padding-inline: var(--ws-pad, var(--e-pad-control));
  color: var(--e-muted);
}

/* Variations sit under the move they branch from, quieter than the main line. */
.vars {
  grid-column: 1 / -1;
  padding-block: var(--e-gap-tight);
  border-block: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 50%, transparent);
  background: color-mix(in oklab, var(--e-ink) 3%, transparent);
}

.var {
  display: flex;
  flex-wrap: wrap;
  margin: 0;
  padding-inline: var(--e-gap-snug);
}

.vm {
  height: 1.75rem;
  padding-inline: 0.3rem;
  color: var(--e-muted);
  font-size: 0.8125rem;
}

.vm.deep {
  font-style: italic;
}

.none {
  grid-column: 1 / -1;
  margin: 0;
  padding: var(--ws-pad, var(--e-pad-control));
  color: var(--e-muted);
  font: var(--e-type-small);
}

@media (pointer: coarse) {
  .mv,
  .vm {
    height: 2.5rem;
  }
}
</style>
