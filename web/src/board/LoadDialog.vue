<script setup lang="ts">
/**
 * Load a game or position: paste a FEN or PGN, pick a file, or drop one on the page. What the text holds shows before
 * Load is pressed. Paste anywhere on the page, or a .pgn dropped on it, opens the dialog on that text.
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { ESelect } from "@eos-ui/vue/components/ESelect";
import { ETextarea } from "@eos-ui/vue/components/ETextarea";
import { EUpload } from "@eos-ui/vue/components/EUpload";
import ChessBoard from "./ChessBoard.vue";
import { inspectPosition, listGames, play } from "./position";

const emit = defineEmits<{ load: [fen: string, moves: string[]] }>();
const open = defineModel<boolean>("open", { required: true });

const text = ref("");
// The text is read a moment after typing stops, so a long PGN is not parsed on every key.
const read = ref("");
let reading: ReturnType<typeof setTimeout> | undefined;
watch(text, (t) => {
  clearTimeout(reading);
  if (!t.trim()) read.value = "";
  else reading = setTimeout(() => (read.value = t), 180);
});
/** "8 moves" for 16 plies: a move is one turn of each side. */
const fullMoves = (plies: number) => {
  const n = Math.ceil(plies / 2);
  return `${n} move${n === 1 ? "" : "s"}`;
};
// A file of several games lists them, and the one picked is what loads.
const games = computed(() => listGames(read.value));
const game = ref("0");
watch(read, () => (game.value = "0"));
const gameOptions = computed(() =>
  games.value.map((g, i) => ({
    value: String(i),
    label: g.label,
    description: `${g.result} · ${fullMoves(g.moves)}`,
  })),
);
const inspected = computed(() => inspectPosition(read.value, Number(game.value)));
/** What the text holds, when it holds a position. */
const found = computed(() => ("line" in inspected.value ? inspected.value : undefined));
const error = computed(() => ("error" in inspected.value ? inspected.value.error : ""));
const preview = computed(() => (found.value ? play(found.value.line) : undefined));
const facts = computed(() => {
  const f = found.value;
  const p = preview.value;
  if (!f || !p) return "";
  const n = f.line.moves.length;
  const moves = f.kind === "pgn" && n ? `${fullMoves(n)}. ` : "";
  return `${moves}${p.turn === "white" ? "White" : "Black"} to move${p.check ? ", in check" : ""}.`;
});
// Reading is pending while the text and what was read differ.
const pending = computed(() => text.value.trim() !== read.value.trim());
const ready = computed(() => !pending.value && !!found.value);

/** A text from the clipboard or a file: opens the dialog on it, so what it holds shows before it loads. */
function offer(t: string) {
  text.value = t;
  read.value = t;
  open.value = true;
}
const picked = ref<File[]>([]);
watch(picked, async (files) => {
  const file = files[0];
  if (!file) return;
  offer(await file.text());
  picked.value = [];
});

const looksLoadable = (t: string) => t.length < 2_000_000 && !("error" in inspectPosition(t));
function onPaste(e: ClipboardEvent) {
  if (
    e.defaultPrevented ||
    (e.target instanceof Element && e.target.closest("input, textarea, select, [contenteditable]"))
  )
    return;
  const t = e.clipboardData?.getData("text/plain") ?? "";
  if (t.trim() && looksLoadable(t)) {
    e.preventDefault();
    offer(t);
  }
}
function onDragOver(e: DragEvent) {
  if (e.dataTransfer?.types.includes("Files")) e.preventDefault();
}
async function onDrop(e: DragEvent) {
  const file = e.dataTransfer?.files[0];
  if (!file) return;
  e.preventDefault();
  offer(await file.text());
}
onMounted(() => {
  window.addEventListener("paste", onPaste);
  window.addEventListener("dragover", onDragOver);
  window.addEventListener("drop", onDrop);
});
onBeforeUnmount(() => {
  clearTimeout(reading);
  window.removeEventListener("paste", onPaste);
  window.removeEventListener("dragover", onDragOver);
  window.removeEventListener("drop", onDrop);
});

function load() {
  const line = found.value?.line;
  if (!line || pending.value) return;
  open.value = false;
  text.value = "";
  read.value = "";
  emit("load", line.fen, line.moves);
}
</script>

<template>
  <EDialog
    v-model:open="open"
    title="Load a game or position"
    description="Paste a FEN or a PGN from Lichess, Chess.com or anywhere, or drop a .pgn file on the board."
  >
    <div class="paste">
      <EUpload v-model="picked" label="PGN file" accept=".pgn,.txt" :preview="false" />
      <ESelect v-if="games.length" v-model="game" label="Game in the file" size="sm" :options="gameOptions" />
      <ETextarea
        v-model="text"
        label="FEN or PGN"
        :rows="4"
        :error="!pending && error ? error : undefined"
        :hint="found?.note"
        placeholder="1. e4 e5 2. Nf3 Nc6 …"
        data-autofocus
        @keydown.ctrl.enter.prevent="load"
        @keydown.meta.enter.prevent="load"
      />
      <!-- The result of reading it, so the person sees what Load will do before pressing it. -->
      <div class="preview" :class="{ 'preview--on': ready }" aria-live="polite">
        <div class="preview-board">
          <ChessBoard v-if="preview" still :position="preview" />
        </div>
        <p class="preview-facts">{{ ready ? facts : "Paste or type to see the position." }}</p>
      </div>
    </div>
    <template #actions>
      <EButton :disabled="!ready" @click="load">Load</EButton>
      <EButton variant="outline" @click="open = false">Cancel</EButton>
    </template>
  </EDialog>
</template>

<style scoped>
.paste {
  display: grid;
  gap: var(--e-gap-base);
}

.preview {
  display: grid;
  grid-template-columns: 6.5rem minmax(0, 1fr);
  align-items: center;
  gap: var(--e-gap-base);
  min-height: 6.5rem;
}

.preview-board {
  width: 6.5rem;
  aspect-ratio: 1;
  border-radius: var(--e-radius-sm);
  background: color-mix(in oklab, var(--e-ink) 6%, var(--e-surface));
  box-shadow: inset 0 0 0 1px var(--e-border);
  opacity: 0.5;
  transition: opacity 0.2s;
}

.preview--on .preview-board {
  opacity: 1;
}

.preview-facts {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
