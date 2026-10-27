<script setup lang="ts">
import { PhArrowLeft } from "@phosphor-icons/vue";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";

/** The sheet's Back button, shown in its `leading` slot when the page before was another sheet. */
defineProps<{ show: boolean }>();
defineEmits<{ back: [] }>();
</script>

<template>
  <EIconButton v-if="show" :icon="PhArrowLeft" label="Back" tooltip size="sm" @click="$emit('back')" />
</template>

<style>
/* Every sheet mounts this, so it carries the frame's one global rule: a long title (a player's name) stays on one line
   and ends in an ellipsis before the Close button; useSheet puts the full text in its title attribute. */
.e-sheet-title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* Below 75rem a sheet is the whole window: its content keeps one readable line length (42rem), level with the title,
   so all six sheets match. Beside the board the sheet is narrower than this and nothing changes. */
.e-sheet-body > * {
  max-inline-size: 42rem;
}
</style>
