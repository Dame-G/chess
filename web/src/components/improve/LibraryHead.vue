<script setup lang="ts">
/** The top of a library pane: the title with one action beside it, and the search and filters below. */
defineProps<{ title: string }>();
</script>

<template>
  <div class="lib-head">
    <div class="lib-top">
      <h1 class="lib-title">{{ title }}</h1>
      <slot name="action" />
    </div>
    <slot />
  </div>
</template>

<style scoped>
.lib-head {
  display: grid;
  flex: 1;
  grid-template-columns: minmax(0, 1fr);
  min-width: 0;
  gap: var(--e-gap-base);
  padding-block: var(--e-gap-snug);
}

.lib-top {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--e-gap-snug);
}

.lib-title {
  margin: 0;
  font: var(--e-type-h5);
}
</style>
