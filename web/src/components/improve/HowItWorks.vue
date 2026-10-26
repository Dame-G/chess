<script setup lang="ts">
import Term from "../Term.vue";

/** The loop in three steps, and with `detail`, how a test and a tune work. */
defineProps<{ detail?: boolean }>();

const loop = [
  {
    title: "Make a change",
    text: "Ask Claude for one, or write it yourself. It becomes a branch: a copy of the engine with that one change.",
  },
  { title: "Test it here", text: "The branch plays against main until the result is clear." },
  { title: "If it passes, merge it", text: "Into main, in the Reckless repo. A failed test costs nothing." },
];
const funnel = [
  { title: "Fingerprint", text: "Seconds. Fixed positions show whether play really changed." },
  { title: "Quick screen", text: "Minutes. Very fast games throw out ideas that clearly lose." },
  {
    title: "Short games, then long",
    text: "Up to hours. Thousands of games, until the statistics say stronger or not.",
  },
];
</script>

<template>
  <div class="how">
    <ol class="steps">
      <li v-for="(s, i) in loop" :key="s.title" class="step">
        <span class="n num" aria-hidden="true">{{ i + 1 }}</span>
        <span class="body">
          <strong>{{ s.title }}</strong>
          {{ s.text }}
        </span>
      </li>
    </ol>
    <template v-if="detail">
      <section class="part">
        <h3 class="part-title">What a test does</h3>
        <ol class="steps">
          <li v-for="(s, i) in funnel" :key="s.title" class="step">
            <span class="n num" aria-hidden="true">{{ i + 1 }}</span>
            <span class="body">
              <strong>{{ s.title }}</strong>
              {{ s.text }}
            </span>
          </li>
        </ol>
      </section>
      <section class="part">
        <h3 class="part-title">What a tune is</h3>
        <p>
          Reckless searches with dozens of constants, and nobody knows their best values. A tune finds them by trial:
          <Term of="spsa" inline>SPSA</Term> nudges all of them at once, plays a few
          <Term of="pairs" inline>game pairs</Term> between a slightly higher and a slightly lower copy, and moves each
          towards whichever side won.
        </p>
        <p>
          It is a robot that writes a branch by adjusting numbers, so its result is tested like any other change.
          Applying a tune writes the values into a new branch and queues that test, at
          <Term of="bounds" inline>[0, 4]</Term>.
        </p>
      </section>
    </template>
  </div>
</template>

<style scoped>
.how {
  display: grid;
  gap: var(--e-gap-loose);
  max-width: 60ch;
}

.steps {
  display: grid;
  gap: var(--e-gap-snug);
  margin: 0;
  padding: 0;
  list-style: none;
}

.step {
  display: grid;
  grid-template-columns: 1.25rem minmax(0, 1fr);
  gap: var(--e-gap-snug);
  font: var(--e-type-small);
}

.n {
  color: var(--e-primary);
  font-weight: 700;
}

.body {
  color: var(--e-muted);
}

.body strong {
  display: block;
  color: var(--e-ink);
  font-weight: 600;
}

.part {
  display: grid;
  gap: var(--e-gap-snug);
}

.part-title {
  margin: 0;
  font: var(--e-type-h6);
}

.part p {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
