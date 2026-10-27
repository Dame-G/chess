<script setup lang="ts">
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";

/**
 * What a profile looks like before its games arrive: the ratings, the filters, the key figures and the chart, so the
 * sheet does not jump when the real parts take their place.
 */
defineProps<{ label?: string }>();
</script>

<template>
  <div class="sk" aria-busy="true">
    <ESkeleton :label="label ?? 'Loading the profile'" :height="44" />
    <ESkeleton :height="28" :width="280" />
    <div class="figs-sk">
      <ESkeleton v-for="n in 4" :key="n" :height="40" />
    </div>
    <ESkeleton :height="170" />
    <ESkeleton :lines="3" />
  </div>
</template>

<style scoped>
.sk {
  display: grid;
  gap: var(--e-gap-base);
}

.figs-sk {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(6.5rem, 1fr));
  gap: var(--e-gap-snug) var(--e-gap-base);
}
</style>
