<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { PhArrowLeft, PhX } from "@phosphor-icons/vue";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";
import { useBoard } from "../board/board";

/**
 * A sheet: a profile or settings, opened in the panel's place beside the board on a wide screen (the board stays visible and
 * live) and over the whole window on a narrow one. It takes focus when it opens and gives it back when it closes;
 * Escape and Close return to the board as it was, Back goes to the previous sheet when there is one.
 */
defineProps<{ title: string; description?: string }>();

const router = useRouter();
const board = useBoard();
const heading = ref<HTMLElement>();
const root = ref<HTMLElement>();
const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;

/** Whether the page before this one was another sheet, so Back can go there. */
const canGoBack = ref(false);
onMounted(async () => {
  const back = (window.history.state as { back?: string | null } | null)?.back ?? null;
  canGoBack.value = !!back && !back.startsWith("/?") && back !== "/";
  await nextTick();
  heading.value?.focus({ preventScroll: true });
});

function close() {
  void router.push({ path: "/", query: board.address });
}
function back() {
  if (canGoBack.value) router.back();
  else close();
}

/** Escape closes the sheet, unless something inside it (a menu, a dialog) takes it first. */
function onKey(e: KeyboardEvent) {
  if (e.key !== "Escape" || e.defaultPrevented) return;
  if (document.querySelector('[aria-modal="true"]')) return;
  e.preventDefault();
  close();
}
onMounted(() => document.addEventListener("keydown", onKey));
onBeforeUnmount(() => {
  document.removeEventListener("keydown", onKey);
  if (opener?.isConnected) opener.focus({ preventScroll: true });
});
</script>

<template>
  <section ref="root" class="sheet" aria-labelledby="sheet-title">
    <header class="sheet-head">
      <EIconButton v-if="canGoBack" :icon="PhArrowLeft" label="Back" tooltip size="sm" @click="back" />
      <div class="sheet-titles">
        <h1 id="sheet-title" ref="heading" class="sheet-title" tabindex="-1">{{ title }}</h1>
        <p v-if="description" class="sheet-description">{{ description }}</p>
      </div>
      <EIconButton :icon="PhX" label="Close (Esc)" tooltip size="sm" @click="close" />
    </header>
    <div class="sheet-body">
      <slot />
    </div>
  </section>
</template>

<style scoped>
.sheet {
  display: grid;
  grid-template-rows: auto minmax(0, 1fr);
  height: 100%;
  min-height: 0;
  background: var(--e-surface);
}

.sheet-head {
  display: flex;
  align-items: flex-start;
  gap: var(--e-gap-snug);
  padding: var(--e-gap-snug) var(--ws-pad);
  border-block-end: var(--line);
}

.sheet-titles {
  flex: 1 1 auto;
  min-width: 0;
  padding-block: 0.375rem;
}

.sheet-title {
  margin: 0;
  font: var(--e-type-h3, 600 1.25rem/1.2 var(--e-font-sans));
}

.sheet-title:focus {
  outline: none;
}

.sheet-title:focus-visible {
  outline: var(--e-stroke-focus) solid var(--e-focus-ring);
  outline-offset: 2px;
}

.sheet-description {
  margin: 0.25rem 0 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.sheet-body {
  overflow-y: auto;
  overscroll-behavior: contain;
  padding: var(--ws-pad);
}

/* A phone gets a 16px gutter. */
@media (width < 36rem) {
  .sheet-head,
  .sheet-body {
    padding-inline: 1rem;
  }
}
</style>
