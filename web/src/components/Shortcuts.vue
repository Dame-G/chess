<script setup lang="ts">
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { EKbd } from "@eos-ui/vue/components/EKbd";

const open = defineModel<boolean>("open", { default: false });

/** Each group is a place; a shortcut is the keys that do it. Keep in step with the `useShortcuts` calls of the board, Improve and Versions. */
const groups = [
  {
    title: "Anywhere",
    rows: [
      { keys: [["Ctrl", "K"]], does: "Search places, tests, tunes and versions" },
      { keys: [["?"]], does: "Show these shortcuts" },
      { keys: [["Esc"]], does: "Close what is open" },
    ],
  },
  {
    title: "Board",
    rows: [
      { keys: [["←"], ["→"]], does: "Step back and forward" },
      { keys: [["Home"], ["End"]], does: "Jump to the first and last move (↑ ↓ too)" },
      { keys: [["Space"]], does: "Play or pause" },
      { keys: [["F"]], does: "Flip the board" },
      { keys: [["E"]], does: "Turn the engine on or off" },
      { keys: [["X"], ["Shift", "X"]], does: "Next and previous mistake, in a reviewed game" },
      { keys: [["N"], ["P"]], does: "Next and previous game or match" },
      { keys: [["L"]], does: "Back to the live match (End does too)" },
      { keys: [["M"]], does: "Watch two versions play" },
    ],
  },
  {
    title: "Improve and Versions",
    rows: [{ keys: [["↑"], ["↓"]], does: "Open the previous and next in the list" }],
  },
] as const;
</script>

<template>
  <EDialog
    v-model:open="open"
    title="Keyboard shortcuts"
    description="Keys work whenever you are not typing."
    size="md"
  >
    <p class="touch">Shortcuts are for a keyboard. On a touch screen every one of these has a button.</p>
    <div class="groups">
      <section v-for="(g, n) in groups" :key="g.title" class="group" :aria-labelledby="`sc-${n}`">
        <h3 :id="`sc-${n}`" class="part-title">{{ g.title }}</h3>
        <dl class="rows">
          <div v-for="r in g.rows" :key="r.does" class="row">
            <dt class="keys">
              <template v-for="(combo, i) in r.keys" :key="combo.join('+')">
                <span v-if="i" class="keys-or" aria-hidden="true">/</span>
                <span class="combo"
                  ><EKbd v-for="k in combo" :key="k">{{ k }}</EKbd></span
                >
              </template>
            </dt>
            <dd class="does">{{ r.does }}</dd>
          </div>
        </dl>
      </section>
    </div>
  </EDialog>
</template>

<style scoped>
/* A phone has no keys to press: say so instead of listing them. */
.touch {
  display: none;
  margin: 0;
  color: var(--e-muted);
}

@media (hover: none) and (pointer: coarse) {
  .touch {
    display: block;
  }

  .groups {
    display: none;
  }
}

.groups {
  display: grid;
  gap: var(--e-gap-section);
}

.group {
  display: grid;
  gap: var(--e-gap-snug);
}

.rows {
  display: grid;
  margin: 0;
}

.row {
  display: grid;
  grid-template-columns: 8rem minmax(0, 1fr);
  align-items: center;
  gap: var(--e-gap-base);
  padding-block: var(--e-gap-snug);
  border-block-end: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 55%, transparent);
}

.row:last-child {
  border-block-end: 0;
}

.keys {
  display: flex;
  align-items: center;
  gap: var(--e-gap-tight);
  margin: 0;
}

.combo {
  display: inline-flex;
  gap: var(--e-gap-tight);
}

.keys-or {
  color: var(--e-muted);
}

.does {
  margin: 0;
}
</style>
