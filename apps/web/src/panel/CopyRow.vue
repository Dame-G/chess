<script setup lang="ts">
import { computed } from "vue";
import { ECopyButton } from "@eos-ui/vue/components/ECopyButton";

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

/** The preview is the text on one line, as far as it fits. */
const preview = computed(() => (typeof props.text === "function" ? props.text() : props.text).replace(/\s+/g, " "));
</script>

<template>
  <div class="row">
    <span class="what">{{ what }}</span>
    <span class="text num" :title="preview">{{ preview }}</span>
    <ECopyButton
      :text="text"
      :label="`Copy ${what}`"
      :copied-label="`${what} copied`"
      show-label
      variant="outline"
      size="sm"
      class="btn"
      :title="`Copy the ${what} of this position to the clipboard`"
    />
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
}
</style>
