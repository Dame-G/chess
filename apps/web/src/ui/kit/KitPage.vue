<script setup lang="ts">
import { EToastProvider } from "@eos-ui/vue/components/EToastProvider";
import { choice, type Choice } from "../../scheme";
import UiConfirm from "../UiConfirm.vue";
import UiSeg from "../UiSeg.vue";
import KitBasics from "./KitBasics.vue";
import KitData from "./KitData.vue";
import KitFormat from "./KitFormat.vue";
import KitOverlays from "./KitOverlays.vue";
import KitTokens from "./KitTokens.vue";

/**
 * The kit page, at `/?fixtures&kit`: every `src/ui` component in every state, and the scales they are made from.
 * Implementers copy from it; the cohesion pass checks screens against it. It is its own app root, loaded only when
 * fixtures are on, so it is not part of the app's main path.
 */
const schemes: { value: Choice; label: string }[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

const nav = [
  ["tokens", "Tokens"],
  ["stat", "Stat"],
  ["badge", "Badge"],
  ["result", "Result"],
  ["score", "Score"],
  ["player", "Player"],
  ["live", "Live"],
  ["seg", "Seg"],
  ["section", "Section"],
  ["keyvalue", "Key value"],
  ["table", "Table"],
  ["empty", "Empty"],
  ["toolbar", "Toolbar"],
  ["field", "Field"],
  ["utilities", "Utilities"],
  ["panelhead", "Panel head"],
  ["dialog", "Dialog"],
  ["sheet", "Sheet page"],
  ["format", "Formatters"],
] as const;
</script>

<template>
  <EToastProvider>
    <div class="kit">
      <header class="kit-head">
        <h1 class="kit-title">Dameless UI kit</h1>
        <UiSeg v-model="choice" label="Colour scheme" size="sm" :options="schemes" />
        <a class="ui-link kit-back" href="/?fixtures">Back to the app</a>
      </header>
      <div class="kit-body">
        <nav class="kit-nav" aria-label="Kit sections">
          <a v-for="[id, label] in nav" :key="id" class="kit-nav-link" :href="`#${id}`">{{ label }}</a>
        </nav>
        <main class="kit-main">
          <KitTokens />
          <KitBasics />
          <KitData />
          <KitOverlays />
          <KitFormat />
        </main>
      </div>
    </div>
    <UiConfirm />
  </EToastProvider>
</template>

<style scoped>
.kit {
  min-height: 100dvh;
  background: var(--c-pane);
  color: var(--c-ink);
}

.kit-head {
  position: sticky;
  inset-block-start: 0;
  z-index: var(--e-z-header);
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--sp-3);
  min-height: var(--h-header);
  padding: var(--sp-2) var(--sp-4);
  border-block-end: var(--e-stroke-hair) solid var(--c-line);
  background: var(--c-pane);
}

.kit-title {
  margin: 0;
  margin-inline-end: auto;
  font: var(--t-title);
}

.kit-back {
  font: var(--t-body);
}

.kit-body {
  display: grid;
  grid-template-columns: 10rem minmax(0, 1fr);
  gap: var(--sp-5);
  max-width: 80rem;
  margin-inline: auto;
  padding: var(--sp-4);
}

.kit-nav {
  position: sticky;
  inset-block-start: calc(var(--h-header) + var(--sp-4));
  display: grid;
  align-content: start;
  align-self: start;
  max-height: calc(100dvh - var(--h-header) - var(--sp-5));
  overflow-y: auto;
}

.kit-nav-link {
  display: block;
  padding: var(--sp-1) var(--sp-2);
  border-radius: var(--r-1);
  color: var(--c-ink-2);
  font: var(--t-body);
  text-decoration: none;
}

.kit-nav-link:hover {
  background: var(--c-raise);
  color: var(--c-ink);
}

.kit-main {
  display: grid;
  gap: var(--sp-5);
  min-width: 0;
}

@media (width < 48rem) {
  .kit-body {
    grid-template-columns: minmax(0, 1fr);
    padding: var(--sp-3);
  }

  .kit-nav {
    position: static;
    display: flex;
    flex-wrap: wrap;
    max-height: none;
  }
}
</style>
