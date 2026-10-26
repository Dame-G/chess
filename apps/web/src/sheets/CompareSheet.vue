<script setup lang="ts">
import { computed } from "vue";
import { useRoute } from "vue-router";
import SheetFrame from "./SheetFrame.vue";

/** Profiles side by side (`GET /api/compare`): a placeholder until the comparison arrives. */
const route = useRoute();
const players = computed(() =>
  String(route.query["players"] ?? "you,dameless")
    .split(",")
    .filter(Boolean)
    .map((p) => (p.startsWith("lichess:") ? p.slice(8) : p === "you" ? "You" : p === "dameless" ? "Dameless" : p)),
);
</script>

<template>
  <SheetFrame title="Compare" description="Players side by side over their reviewed games, and who leads each row.">
    <p class="muted">Comparing {{ players.join(", ") }}. The columns come here.</p>
  </SheetFrame>
</template>

<style scoped>
.muted {
  margin: 0;
  color: var(--e-muted);
}
</style>
