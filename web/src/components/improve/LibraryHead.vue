<script setup lang="ts">
/** The top of a library pane: the title with its actions beside it. One bar tall, like the heads of the other panes. */
defineProps<{ title: string }>();
</script>

<template>
  <div class="lib-top">
    <h1 class="lib-title">{{ title }}</h1>
    <slot name="action" />
  </div>
</template>

<style scoped>
.lib-top {
  display: flex;
  flex: 1;
  justify-content: space-between;
  align-items: center;
  gap: var(--e-gap-snug);
  min-width: 0;
}

.lib-title {
  margin: 0;
  font: var(--e-type-h5);
}
</style>
