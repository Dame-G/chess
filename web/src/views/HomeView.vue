<script setup lang="ts">
import { useRouter } from "vue-router";
import { PhGraph } from "@phosphor-icons/vue";
import { EActions } from "@eos-ui/vue/components/EActions";
import { EButton } from "@eos-ui/vue/components/EButton";
import ChequerField from "../components/home/ChequerField.vue";

const router = useRouter();
</script>

<template>
  <div class="cover">
    <ChequerField />
    <div class="cover-body">
      <h1 class="cover-title">Reckless <em>Lab</em></h1>
      <p class="cover-tagline">Make a chess engine stronger, one tested idea at a time.</p>
      <EActions class="cover-actions">
        <EButton :icon="PhGraph" @click="router.push('/board')">Open the board</EButton>
        <EButton variant="outline" @click="router.push('/lab')">Open the lab</EButton>
      </EActions>
    </div>
  </div>
</template>

<style scoped>
/* Fills the shell body (which is positioned) from header to footer, so the page never scrolls on a desktop. */
.cover {
  position: absolute;
  inset: 0;
  display: grid;
  overflow: hidden auto;
}

.cover-body {
  position: relative;
  display: grid;
  align-content: center;
  justify-items: center;
  gap: var(--e-gap-base);
  padding: var(--e-gap-section) var(--e-gutter);
  text-align: center;
}

.cover-title {
  margin: 0;
  color: var(--e-ink);
  font:
    600 clamp(3rem, 9vw, 6.5rem) / 1 "Fraunces Variable",
    serif;
  letter-spacing: -0.02em;
}

.cover-title em {
  color: var(--e-primary);
  font-style: italic;
}

.cover-tagline {
  max-width: 44ch;
  text-wrap: balance;
  margin: 0;
  font: var(--e-type-deck);
  font-weight: var(--e-weight-medium);
}

.cover-actions {
  justify-content: center;
  margin-top: var(--e-gap-snug);
}
</style>
