<script setup lang="ts">
import Term from "../Term.vue";
</script>

<template>
  <div class="explain">
    <p>
      Reckless searches with dozens of constants: how soon to give up on a bad line, how much to trust a shallow result.
      Nobody knows their best values, so a tune finds them by trial. <Term of="spsa" inline>SPSA</Term> nudges all of
      them at once, plays a few <Term of="pairs" inline>game pairs</Term> between a slightly higher and a slightly lower
      copy, and moves each constant towards whichever side won. Over thousands of games they drift towards stronger
      values.
    </p>
    <p>
      A tune only suggests. When it looks settled, apply it: the values go into a new branch, and an experiment tests
      that branch against main at <Term of="bounds" inline>[0, 4]</Term>. Only a pass makes the change worth keeping.
    </p>
  </div>
</template>

<style scoped>
.explain {
  display: grid;
  gap: var(--e-gap-base);
  max-width: 60ch;
}

.explain p {
  margin: 0;
}
</style>
