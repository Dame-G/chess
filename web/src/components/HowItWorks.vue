<script setup lang="ts">
/** The funnel in four lines. Kept short: the stages themselves explain the rest. */
const steps = [
  {
    title: "Change the code",
    text: "Each idea is a branch of the Reckless code: a search trick, a tweak, a clean-up.",
  },
  { title: "Check the fingerprint", text: "Seconds. Fixed positions show whether play really changed." },
  { title: "Quick screen", text: "Minutes. Very fast games throw out ideas that clearly lose." },
  { title: "Prove it", text: "Up to hours. Thousands of games, until the statistics say stronger or not." },
];
</script>

<template>
  <ol class="how">
    <li v-for="(s, i) in steps" :key="s.title" class="how-step">
      <span class="how-n num" aria-hidden="true">{{ i + 1 }}</span>
      <span class="how-body">
        <strong>{{ s.title }}</strong>
        {{ s.text }}
      </span>
    </li>
  </ol>
</template>

<style scoped>
.how {
  display: grid;
  gap: var(--e-gap-snug);
  margin: 0;
  padding: 0;
  list-style: none;
}

.how-step {
  display: grid;
  grid-template-columns: 1.25rem minmax(0, 1fr);
  gap: var(--e-gap-snug);
  font: var(--e-type-small);
}

.how-n {
  color: var(--e-primary);
  font-weight: 700;
}

.how-body {
  color: var(--e-muted);
}

.how-body strong {
  display: block;
  color: var(--e-ink);
  font-weight: 600;
}
</style>
