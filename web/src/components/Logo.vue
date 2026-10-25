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
  gap: 0.5rem;
}

/* A squared tile on a two-by-two chequer: the board, reduced to its pattern. */
.logo-tile {
  display: grid;
  place-items: center;
  width: 1.5rem;
  height: 1.5rem;
  border-radius: 2px;
  background: conic-gradient(
      var(--e-primary) 25%,
      color-mix(in oklab, var(--e-primary) 72%, black) 0 50%,
      var(--e-primary) 0 75%,
      color-mix(in oklab, var(--e-primary) 72%, black) 0
    )
    0 0 / 50% 50%;
  color: #fff8ee;
  font-size: 1rem;
}

.logo-word {
  font:
    600 var(--e-text-md) / 1 "Fraunces Variable",
    Georgia,
    serif;
  letter-spacing: -0.015em;
  white-space: nowrap;
}

@media (width < 36rem) {
  .logo-word {
    display: none;
  }
}

.logo-lab {
  color: var(--e-primary);
  font-style: italic;
}
</style>
