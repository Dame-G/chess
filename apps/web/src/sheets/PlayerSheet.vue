<script setup lang="ts">
import { computed } from "vue";
import { useRoute } from "vue-router";
import SheetFrame from "./SheetFrame.vue";

/** A player's profile (`GET /api/players/{key}/profile`): a placeholder until the profile arrives. */
const route = useRoute();
const key = computed(() => String(route.params["key"] ?? ""));
const name = computed(() => (key.value.startsWith("lichess:") ? key.value.slice(8) : key.value));
</script>

<template>
  <SheetFrame :title="name" description="Their games added up: results, accuracy by phase, openings and time.">
    <p class="muted">The profile of {{ name }} comes here.</p>
  </SheetFrame>
</template>

<style scoped>
.muted {
  margin: 0;
  color: var(--e-muted);
}
</style>
