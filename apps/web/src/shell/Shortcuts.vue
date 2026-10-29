<script setup lang="ts">
import { EKbd } from "@eos-ui/vue/components/EKbd";
import { formatShortcut } from "@eos-ui/vue";
import UiDialog from "../ui/UiDialog.vue";
import UiKeyValue from "../ui/UiKeyValue.vue";
import UiKv from "../ui/UiKv.vue";
import UiSection from "../ui/UiSection.vue";
import { useUi } from "./ui";

/** The `?` dialog. Keep it in step with `useKeys` in `keys.ts`. */
const ui = useUi();
const mod = formatShortcut("mod+k");

/** Each row is what a key does, and the keys (alternatives side by side, a combination as its keys in order). */
const groups = [
  {
    title: "Anywhere",
    rows: [
      { keys: [mod], does: "Open the command bar" },
      { keys: [["N"]], does: "New game" },
      { keys: [["D"]], does: "Start today's drills" },
      { keys: [["?"]], does: "Show these shortcuts" },
      { keys: [["Esc"]], does: "Close what is open" },
    ],
  },
  {
    title: "Board",
    rows: [
      { keys: [["←"], ["→"]], does: "Step back and forward" },
      { keys: [["Home"], ["End"]], does: "First and last move" },
      { keys: [["F"]], does: "Flip the board" },
      { keys: [["Space"]], does: "Engine on or off, in Explore" },
    ],
  },
  {
    title: "Review",
    rows: [{ keys: [["X"], ["Shift", "X"]], does: "Next and previous mistake" }],
  },
  {
    title: "Sheets",
    rows: [
      { keys: [["G", "Y"]], does: "Go to You" },
      { keys: [["G", "D"]], does: "Go to Dameless" },
      { keys: [["G", "S"]], does: "Go to Settings" },
    ],
  },
];
</script>

<template>
  <UiDialog v-model:open="ui.shortcuts" title="Keyboard shortcuts" size="lg" plain>
    <div class="shortcuts" tabindex="-1" data-autofocus>
      <p class="touch">Shortcuts are for a keyboard. On a touch screen every one of these has a button.</p>
      <div class="groups">
        <UiSection v-for="g in groups" :key="g.title" :title="g.title" :level="2">
          <UiKeyValue dense>
            <UiKv v-for="r in g.rows" :key="r.does" :label="r.does">
              <span class="keys">
                <template v-for="(combo, i) in r.keys" :key="combo.join('+')">
                  <span v-if="i" class="or" aria-hidden="true">/</span>
                  <span class="combo"
                    ><EKbd v-for="k in combo" :key="k">{{ k }}</EKbd></span
                  >
                </template>
              </span>
            </UiKv>
          </UiKeyValue>
        </UiSection>
      </div>
    </div>
  </UiDialog>
</template>

<style scoped>
.touch {
  display: none;
  margin: 0 0 var(--sp-3);
  color: var(--c-ink-2);
}

@media (hover: none) and (pointer: coarse) {
  .touch {
    display: block;
  }
}

.shortcuts {
  container-type: inline-size;
  min-width: 0;
}

/* Nothing here takes input: focus rests on the list itself, so the dialog does not open on its close button. */
.shortcuts:focus {
  outline: none;
}

.groups {
  display: grid;
  gap: var(--sp-4);
  min-width: 0;
}

@container (width >= 34rem) {
  .groups {
    grid-template-columns: repeat(2, minmax(0, 1fr));
    column-gap: var(--sp-5);
    align-items: start;
  }
}

.keys {
  display: inline-flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--sp-1);
}

.combo {
  display: inline-flex;
  gap: var(--sp-1);
}

.or {
  color: var(--c-ink-3);
}
</style>
