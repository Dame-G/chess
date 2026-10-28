<script setup lang="ts">
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { EKbd } from "@eos-ui/vue/components/EKbd";
import { formatShortcut } from "@eos-ui/vue";
import { useUi } from "./ui";

/** The `?` dialog. Keep it in step with `useKeys` in `keys.ts`. */
const ui = useUi();
const mod = formatShortcut("mod+k");

const groups = [
  {
    title: "Anywhere",
    rows: [
      { keys: [mod], does: "Open the command bar" },
      { keys: [["N"]], does: "New game" },
      { keys: [["D"]], does: "Start today's drills" },
      {
        keys: [
          ["G", "Y"],
          ["G", "D"],
          ["G", "S"],
        ],
        does: "Go to You, Dameless, Settings",
      },
      { keys: [["?"]], does: "Show these shortcuts" },
      { keys: [["Esc"]], does: "Close what is open" },
    ],
  },
  {
    title: "Board",
    rows: [
      { keys: [["←"], ["→"]], does: "Step back and forward" },
      { keys: [["Home"], ["End"]], does: "Jump to the first and last move" },
      { keys: [["F"]], does: "Flip the board" },
      { keys: [["Space"]], does: "Engine on or off, in Explore" },
      { keys: [["X"], ["Shift", "X"]], does: "Next and previous mistake, in Review" },
    ],
  },
];
</script>

<template>
  <EDialog
    v-model:open="ui.shortcuts"
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
                <span v-if="i" class="or" aria-hidden="true">/</span>
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
.touch {
  display: none;
  margin: 0;
  color: var(--e-muted);
}

@media (hover: none) and (pointer: coarse) {
  .touch {
    display: block;
  }
}

.groups {
  display: grid;
  gap: var(--e-gap-loose);
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
  grid-template-columns: 11rem 1fr;
  align-items: center;
  gap: var(--e-gap-base);
  padding-block: var(--e-gap-snug);
  border-block-end: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 50%, transparent);
}

.keys {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--e-gap-tight);
}

.combo {
  display: inline-flex;
  gap: 0.125rem;
}

.or {
  color: var(--e-muted);
}

.does {
  margin: 0;
}

@media (width < 30rem) {
  .row {
    grid-template-columns: 1fr;
    gap: var(--e-gap-tight);
  }
}
</style>
