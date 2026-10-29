<script setup lang="ts">
import { computed } from "vue";

/**
 * A row (or a wrapping grid) of `UiStat`s. By default as many columns as fit, none narrower than 7rem; `columns` fixes
 * the count. `dense` tightens the gaps for evidence under a chart.
 */
const props = defineProps<{ columns?: number; dense?: boolean }>();

const style = computed(() =>
  props.columns ? { gridTemplateColumns: `repeat(${props.columns}, minmax(0, 1fr))` } : undefined,
);
</script>

<template>
  <div class="ui-stats" :class="{ 'ui-stats--dense': dense }" :style="style">
    <slot />
  </div>
</template>

<style scoped>
.ui-stats {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(7rem, 1fr));
  gap: var(--sp-3) var(--sp-4);
  min-width: 0;
}

.ui-stats--dense {
  gap: var(--sp-2) var(--sp-3);
}
</style>
