<script setup lang="ts">
/**
 * What the explorer and the tablebase say when they cannot answer: not logged in, locked while a game runs on
 * Lichess, rate-limited or failed. Calm and short, with the next step where there is one.
 */
import { RouterLink } from "vue-router";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import type { LookupState } from "./useExplorer";

defineProps<{
  state: Extract<LookupState, "login" | "locked" | "limited" | "error">;
  message?: string;
  what: string;
}>();
</script>

<template>
  <ENotice
    v-if="state === 'login'"
    tone="info"
    :title="`Log in to your Lichess account to use the ${what}`"
    class="note"
  >
    <ELink to="/lichess" :link-component="RouterLink">Log in on the Lichess page</ELink>
  </ENotice>
  <ENotice
    v-else-if="state === 'locked'"
    tone="neutral"
    title="Analysis is off while you're playing on Lichess"
    class="note"
  >
    It comes back when your game ends.
  </ENotice>
  <ENotice v-else-if="state === 'limited'" tone="warning" :title="`Lichess asked us to slow down`" class="note">
    The {{ what }} will answer again in a minute.
  </ENotice>
  <ENotice v-else tone="danger" :title="`The ${what} did not answer`" class="note">{{ message }}</ENotice>
</template>

<style scoped>
.note {
  margin: var(--e-gap-base) var(--ws-pad);
}
</style>
