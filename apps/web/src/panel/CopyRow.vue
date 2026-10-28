<script setup lang="ts">
import { computed } from "vue";
import { PhCheck, PhCopy, PhWarning } from "@phosphor-icons/vue";
import { useCopy } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";

/**
 * One thing the page can copy, said in words: its name (FEN, PGN), a preview of the text, and a button that reads
 * "Copy FEN" and, once pressed, "FEN copied" (spoken as well). The text is read when the button is pressed.
 */
const props = defineProps<{
  /** What is copied, as a short name: "FEN", "PGN". */
  what: string;
  /** The text, or a function that makes it when the button is pressed. */
  text: string | (() => string);
}>();

const { copied, failed, copy } = useCopy({ timeout: 2000 });
const value = computed(() => (typeof props.text === "function" ? props.text() : props.text));
/** The preview is the text on one line, as far as it fits. */
const preview = computed(() => value.value.replace(/\s+/g, " "));
const label = computed(() =>
  failed.value ? `Could not copy ${props.what}` : copied.value ? `${props.what} copied` : `Copy ${props.what}`,
);
const tip = computed(() => `Copy the ${props.what} of this position to the clipboard`);

async function press() {
  await copy(value.value);
}
</script>

<template>
  <div class="row">
    <span class="what">{{ what }}</span>
    <span class="text num" :title="preview">{{ preview }}</span>
    <EButton
      size="sm"
      variant="outline"
      class="btn"
      :class="{ 'btn--done': copied }"
      :icon="failed ? PhWarning : copied ? PhCheck : PhCopy"
      :title="tip"
      :aria-label="label"
      @click="press"
    >
      <span aria-live="polite">{{ label }}</span>
    </EButton>
  </div>
</template>

<style scoped>
.row {
  display: flex;
  align-items: center;
  gap: var(--e-gap-snug);
  min-width: 0;
}

.what {
  flex: none;
  min-inline-size: 3ch;
  color: var(--e-muted);
  font: 700 0.6875rem/1 var(--e-font-mono);
  letter-spacing: 0.04em;
}

.text {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  color: var(--e-muted);
  font: 0.75rem/1.3 var(--e-font-mono);
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* The button keeps one width between "Copy FEN" and "FEN copied", so the preview does not shift. */
.btn {
  flex: none;
  min-inline-size: 7.5rem;
  justify-content: center;
}

.btn--done {
  color: var(--e-success, var(--e-primary));
}
</style>
