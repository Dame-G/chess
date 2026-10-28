<script setup lang="ts">
import { computed, ref } from "vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ECellRow, type CellRowCell } from "@eos-ui/vue/components/ECellRow";
import { EList } from "@eos-ui/vue/components/EList";
import { focusCurrent, followCurrent, stepOf, type Step } from "./follow";
import { rows, type GameTree, type Row, type TreeNode } from "./tree";

/**
 * Explore's move list: the main line in numbered pairs (cells of an `EList`) and, under a move with alternatives, its
 * variations as runs of moves (deeper ones indented), as Lichess shows a study. Any move is a click away.
 */
const props = defineProps<{
  tree: GameTree;
  /** The node on the board. */
  node: TreeNode;
  /** Bumped when the tree changes in place, so the rows are read again. */
  version: number;
}>();
const emit = defineEmits<{ select: [node: TreeNode]; step: [to: Step] }>();

const list = computed(() => {
  void props.version;
  return rows(props.tree);
});

/** The cells of a numbered pair, named by node id; a missing White move is a disabled gap that can never be chosen. */
function cells(row: Extract<Row, { type: "moves" }>): CellRowCell[] {
  const out: CellRowCell[] = [
    row.white ? { value: row.white.id, label: row.white.san } : { value: -row.number - 1, label: "…", disabled: true },
  ];
  if (row.black) out.push({ value: row.black.id, label: row.black.san });
  return out;
}

/** A choice is a node id; the nodes are looked up again in the tree, which keeps them. */
function choose(id: string | number) {
  const found = props.tree.find(Number(id));
  if (found) emit("select", found);
}

const box = ref<InstanceType<typeof EList>>();
/** The arrow keys, Home and End step the board past the end of a row, as they do from anywhere else. */
function onKeys(e: KeyboardEvent) {
  const to = stepOf(e);
  if (!to) return;
  e.preventDefault();
  e.stopPropagation();
  if (e.repeat) return;
  emit("step", to);
  focusCurrent(() => box.value?.$el as HTMLElement | undefined);
}

followCurrent(
  () => box.value?.$el as HTMLElement | undefined,
  () => props.node.id,
);
</script>

<template>
  <EList
    ref="box"
    :model-value="node.id"
    label="Moves and variations"
    flush
    follow-focus
    class="tree"
    @keydown.capture="onKeys"
    @select="choose"
  >
    <template
      v-for="row in list"
      :key="row.type === 'moves' ? `m${row.number}-${row.white?.id}-${row.black?.id}` : row.key"
    >
      <ECellRow v-if="row.type === 'moves'" :label="`${row.number}.`" :columns="2" :cells="cells(row)">
        <template #cell="{ cell }">
          <span class="mv">{{ cell.label }}</span>
        </template>
      </ECellRow>
      <div v-else class="vars">
        <p v-for="(line, i) in row.lines" :key="i" class="var">
          <EButton
            v-for="t in line"
            :key="t.node.id"
            size="xs"
            variant="quiet"
            class="vm"
            :class="{ deep: t.depth > 1 }"
            :aria-current="t.node.id === node.id ? 'true' : undefined"
            @click="emit('select', t.node)"
          >
            {{ t.text }}
          </EButton>
        </p>
      </div>
    </template>
    <p v-if="!list.length" class="none">No moves yet. Move a piece, or paste a FEN or PGN.</p>
  </EList>
</template>

<style scoped>
.tree {
  align-content: start;
  height: 100%;
  overflow-y: auto;
  overscroll-behavior: contain;
}

/* A move is a short word: its cell is the 2rem tall line of a chess client, not the 2.25rem of a form control. Touch
   keeps eos's full target. */
@media (pointer: fine) {
  .tree {
    --e-tap: 2rem;
  }
}

.mv {
  font-family: var(--e-font-mono);
}

/* Variations sit under the move they branch from, quieter than the main line. */
.vars {
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
  color: var(--e-muted);
  font-family: var(--e-font-mono);
  text-decoration: none;
}

.vm[aria-current] {
  background: var(--e-primary-soft);
  color: var(--e-primary);
  font-weight: 700;
}

.vm.deep {
  font-style: italic;
}

.none {
  margin: 0;
  padding: var(--ws-pad, var(--e-pad-control));
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
