<script setup lang="ts">
import { onMounted } from "vue";
import { EListRow } from "@eos-ui/vue/components/EListRow";

/**
 * The last row of a list that has more games on the server. It asks for them as soon as it is rendered, which is when
 * the list has been scrolled near its end; each page's row is a new one, so reaching it again asks again. When the
 * request fails it says so, and choosing the row asks again.
 */
defineProps<{ value: string; loading: boolean; error: string | null; nested?: boolean }>();
const emit = defineEmits<{ more: [] }>();
onMounted(() => emit("more"));
</script>

<template>
  <EListRow :value="value" class="more" :class="{ nested }">
    <template #title>
      <span class="text" :class="{ 'text--error': error }">{{
        error ? "More games did not load. Try again" : loading ? "Loading more games" : "More games"
      }}</span>
    </template>
  </EListRow>
</template>

<style scoped>
.text {
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.text--error {
  color: var(--c-loss);
}
</style>
