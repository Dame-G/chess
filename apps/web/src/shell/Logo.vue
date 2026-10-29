<script setup lang="ts">
import { PhHorse } from "@phosphor-icons/vue";
</script>

<template>
  <span class="logo">
    <span class="logo-tile" aria-hidden="true"><PhHorse weight="fill" /></span>
    <span class="logo-word">Dame<span class="logo-less">less</span></span>
  </span>
</template>

<style scoped>
.logo {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-2);
}

/* A squared tile on a two-by-two chequer: the board, reduced to its pattern. */
.logo-tile {
  display: grid;
  place-items: center;
  width: 1.5rem;
  height: 1.5rem;
  border-radius: var(--r-1);
  background: conic-gradient(
      var(--e-primary) 25%,
      color-mix(in oklab, var(--e-primary) 72%, black) 0 50%,
      var(--e-primary) 0 75%,
      color-mix(in oklab, var(--e-primary) 72%, black) 0
    )
    0 0 / 50% 50%;
  color: var(--c-pip-white);
  font-size: var(--i-md);
}

.logo-word {
  font:
    600 var(--e-text-lg) / 1 "Fraunces Variable",
    Georgia,
    serif;
  letter-spacing: -0.015em;
  white-space: nowrap;
}

@media (width < 30rem) {
  .logo-word {
    display: none;
  }
}

.logo-less {
  color: var(--e-primary);
  font-style: italic;
}
</style>
