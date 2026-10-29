<script setup lang="ts">
import { computed, ref } from "vue";
import UiEmpty from "../ui/UiEmpty.vue";
import { focusCurrent, followCurrent, stepOf, type Step } from "./follow";
import MoveRow from "./MoveRow.vue";
import { rows, type GameTree, type Row, type TreeNode } from "./tree";

/**
 * Explore's move list: the main line in numbered pairs and, under a move with alternatives, its variations as runs of
 * moves (deeper ones indented and italic), as Lichess shows a study. Any move is a click away. The rows look like the
 * game's move list, so a position is read the same in every mode.
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

type MovesRow = Extract<Row, { type: "moves" }>;

/** The node ids of the main line, to tell what is ahead of the board from what is behind it. */
const ahead = computed(() => {
  const ids = new Set<number>();
  for (let n: TreeNode | undefined = props.node.children[0]; n; n = n.children[0]) ids.add(n.id);
  return ids;
});

/** The one move that takes the tab stop: the one on the board, else the first move of the first row. */
const tabId = computed(() => {
  if (props.node.id !== 0) return props.node.id;
  const first = list.value.find((r): r is MovesRow => r.type === "moves");
  return first?.white?.id ?? first?.black?.id ?? -1;
});

function rowProps(row: MovesRow) {
  const w = row.white;
  const b = row.black;
  return {
    number: row.number,
    a: w?.san ?? null,
    b: b?.san ?? null,
    pa: w?.id ?? -1,
    pb: b?.id ?? -1,
    curA: !!w && w.id === props.node.id,
    curB: !!b && b.id === props.node.id,
    futA: !!w && ahead.value.has(w.id),
    futB: !!b && ahead.value.has(b.id),
    tabA: !!w && w.id === tabId.value,
    tabB: !!b && b.id === tabId.value,
  };
}

const box = ref<HTMLElement>();

/** A click on a move goes to it: the nodes are looked up again in the tree, which keeps them. */
function onClick(e: MouseEvent) {
  const id = (e.target as Element).closest<HTMLElement>("[data-id]")?.dataset["id"];
  const found = id === undefined ? undefined : props.tree.find(Number(id));
  if (found) emit("select", found);
}

/** The arrow keys, Home and End step the board past the end of a row, as they do from anywhere else. */
function onKeys(e: KeyboardEvent) {
  const to = stepOf(e);
  if (!to) return;
  e.preventDefault();
  e.stopPropagation();
  if (e.repeat) return;
  emit("step", to);
  focusCurrent(() => box.value);
}

followCurrent(
  () => box.value,
  () => props.node.id,
);
</script>

<template>
  <!-- eslint-disable-next-line vuejs-accessibility/no-static-element-interactions -- the moves inside are buttons: the click is delegated, the keys are theirs -->
  <div ref="box" class="tree" role="group" aria-label="Moves and variations" @click="onClick" @keydown.capture="onKeys">
    <template
      v-for="row in list"
      :key="row.type === 'moves' ? `m${row.number}-${row.white?.id}-${row.black?.id}` : row.key"
    >
      <MoveRow v-if="row.type === 'moves'" v-bind="rowProps(row)" />
      <div v-else class="vars">
        <p v-for="(line, i) in row.lines" :key="i" class="var">
          <button
            v-for="t in line"
            :key="t.node.id"
            type="button"
            class="vm"
            :class="{ deep: t.depth > 1 }"
            :data-id="t.node.id"
            :aria-current="t.node.id === node.id ? 'true' : undefined"
            :tabindex="t.node.id === tabId ? 0 : -1"
          >
            {{ t.text }}
          </button>
        </p>
      </div>
    </template>
    <UiEmpty v-if="!list.length" dense title="No moves yet." body="Move a piece on the board." class="none" />
  </div>
</template>

<style scoped>
.tree {
  min-height: 0;
  height: 100%;
  overflow-y: auto;
  overscroll-behavior: contain;
}

/* Variations sit under the move they branch from, quieter than the main line. */
.vars {
  padding-block: var(--sp-1);
  padding-inline-start: 2.5rem;
  border-block: var(--e-stroke-hair) solid var(--c-line);
  background: var(--c-raise);
}

.var {
  display: flex;
  flex-wrap: wrap;
  margin: 0;
  padding-inline: var(--sp-2);
}

.vm {
  min-height: var(--h-move);
  padding: 0 var(--sp-1);
  border: 0;
  border-radius: var(--r-1);
  background: transparent;
  color: var(--c-ink-2);
  font: var(--t-fig);
  cursor: pointer;
}

.vm:hover {
  background: var(--c-sunk);
  color: var(--c-ink);
}

.vm[aria-current] {
  background: var(--c-sunk);
  box-shadow: inset 0 calc(-1 * var(--e-stroke-focus)) 0 var(--c-act);
  color: var(--c-ink);
  font-weight: 700;
}

.vm:focus-visible {
  outline: var(--focus-ring);
}

.vm.deep {
  font-style: italic;
}

.none {
  padding: var(--sp-3);
}
</style>
