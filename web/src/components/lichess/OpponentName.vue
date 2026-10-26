<script setup lang="ts">
defineProps<{ name: string; title?: string | null; rating?: number | null }>();
</script>

<template>
  <span class="opp">
    <span v-if="title" class="title">{{ title }}</span>
    <span class="name">{{ name }}</span>
    <span v-if="rating" class="rating num">{{ rating }}</span>
  </span>
</template>

<style scoped>
.opp {
  display: flex;
  align-items: baseline;
  gap: var(--e-gap-tight);
  min-inline-size: 0;
  line-height: 1.25rem;
}

.name {
  overflow: hidden;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.title {
  color: var(--e-primary);
  font: var(--e-type-small);
  font-weight: 700;
}

.rating {
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
