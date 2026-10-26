<script setup lang="ts">
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import MeChallenge from "./MeChallenge.vue";
import MeSeek from "./MeSeek.vue";
import { stored } from "../../stored";
import type { MeStatus } from "../../lichess";

defineProps<{ me: MeStatus }>();

const how = stored<"seek" | "challenge">("lichess.me.how", "seek");
const options = [
  { value: "seek", label: "Find a game" },
  { value: "challenge", label: "Challenge" },
];
</script>

<template>
  <section class="play" aria-label="Play">
    <ESegmented v-model="how" label="Play" hide-label :options="options" />
    <MeSeek v-if="how === 'seek'" :me="me" />
    <MeChallenge v-else />
  </section>
</template>

<style scoped>
.play {
  display: grid;
  gap: var(--e-gap-loose);
}
</style>
