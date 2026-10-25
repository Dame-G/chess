<script setup lang="ts">
import { PhFingerprint, PhGitBranch, PhLightning, PhTrophy } from "@phosphor-icons/vue";

const steps = [
  {
    icon: PhGitBranch,
    title: "Change the code",
    text: "Each idea is a branch of the Reckless code: a new search trick, a tweak, a clean-up.",
  },
  {
    icon: PhFingerprint,
    title: "Check the fingerprint",
    text: "Seconds. Both versions search fixed positions; the counts show whether play really changed.",
  },
  {
    icon: PhLightning,
    title: "Quick screen",
    text: "Minutes. Very fast games throw out ideas that clearly lose.",
  },
  {
    icon: PhTrophy,
    title: "Prove it",
    text: "Up to hours. Thousands of games, until the statistics say stronger or not.",
  },
];
</script>

<template>
  <ol class="how">
    <li v-for="(s, i) in steps" :key="s.title" class="how-step">
      <span class="how-icon" aria-hidden="true"><component :is="s.icon" weight="duotone" /></span>
      <span class="how-number num" aria-hidden="true">{{ i + 1 }}</span>
      <strong class="how-title">{{ s.title }}</strong>
      <p class="how-text">{{ s.text }}</p>
    </li>
  </ol>
</template>

<style scoped>
.how {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(12rem, 1fr));
  gap: var(--e-gap-base);
  margin: 0;
  padding: 0;
  counter-reset: step;
  list-style: none;
}

.how-step {
  position: relative;
  display: grid;
  align-content: start;
  gap: var(--e-gap-tight);
  padding: var(--e-gap-loose) var(--e-gap-base) var(--e-gap-base);
  border-radius: var(--e-radius-lg);
  background: var(--e-surface);
  box-shadow: var(--e-shadow-frame);
}

.how-icon {
  display: grid;
  place-items: center;
  width: 2.5rem;
  height: 2.5rem;
  border-radius: var(--e-radius-md);
  background: var(--e-primary-soft);
  color: var(--e-primary);
  font-size: 1.5rem;
}

.how-number {
  position: absolute;
  top: var(--e-gap-base);
  inset-inline-end: var(--e-gap-base);
  color: color-mix(in oklab, var(--e-muted) 60%, transparent);
  font:
    600 var(--e-text-2xl) / 1 "Fraunces Variable",
    serif;
}

.how-title {
  font: var(--e-type-h5);
}

.how-text {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
