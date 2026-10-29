<script setup lang="ts">
import { computed } from "vue";

/**
 * A path or an address on one line, never wrapped: when it does not fit, the middle is cut with an ellipsis and the end
 * (the file or folder that says what it is) stays. The whole value is the tooltip and what is selected and copied.
 */
const props = defineProps<{ text: string }>();

/** The part kept whole at the end: the last folder or file, and a little more. */
const TAIL = 16;
const parts = computed(() => {
  const t = props.text;
  if (t.length <= TAIL * 2) return { head: t, tail: "" };
  return { head: t.slice(0, -TAIL), tail: t.slice(-TAIL) };
});
</script>

<template>
  <span class="path mono" :title="text"
    ><span class="head">{{ parts.head }}</span
    ><span v-if="parts.tail" class="tail">{{ parts.tail }}</span></span
  >
</template>

<style scoped>
.path {
  display: flex;
  min-width: 0;
  white-space: nowrap;
}

.head {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}

.tail {
  flex: none;
}
</style>
