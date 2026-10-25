<script setup lang="ts">
import { PhHorse } from "@phosphor-icons/vue";
</script>

<template>
  <span class="logo">
    <span class="logo-tile" aria-hidden="true"><PhHorse weight="fill" /></span>
    <span class="logo-word">Reckless <span class="logo-lab">Lab</span></span>
  </span>
</template>

<style scoped>
.logo {
  display: inline-flex;
  align-items: center;
  gap: var(--e-gap-snug);
}

.logo-tile {
  display: grid;
  place-items: center;
  width: 2rem;
  height: 2rem;
  border-radius: var(--e-radius-md);
  background: linear-gradient(145deg, var(--e-brand), var(--e-primary));
  box-shadow: inset 0 -2px 0 rgb(0 0 0 / 18%);
  color: #fff8ee;
  font-size: 1.25rem;
}

.logo-word {
  font:
    600 var(--e-text-lg) / 1 "Fraunces Variable",
    Georgia,
    serif;
  letter-spacing: -0.01em;
}

.logo-lab {
  color: var(--e-primary);
  font-style: italic;
}
</style>
