<script setup lang="ts">
import { EButton } from "@eos-ui/vue/components/EButton";
import { PhArrowLeft, PhX } from "@phosphor-icons/vue";
import {
  computed,
  getCurrentInstance,
  inject,
  nextTick,
  useId,
  onBeforeUnmount,
  onMounted,
  useTemplateRef,
  watch,
} from "vue";
import { routeLocationKey, routerKey } from "vue-router";
import { up, useMedia } from "../media";
import UiEmpty from "./UiEmpty.vue";
import { captureOpener, returnToOpener } from "./opener";
import { useSheetNav } from "./useSheetNav";

export interface SheetTab {
  id: string;
  label: string;
  count?: number | string;
}

/**
 * The frame every sheet uses. A header that stays (back, title with its subtitle, your actions, close), a hero strip
 * that scrolls away (`#lead`), a tab row that sticks under the header, and the tab's content, which scrolls. Beside the
 * board it docks to the right at `--w-sheet` (`wide`: `--w-sheet-wide`); below 75rem it covers the window and the
 * close button is a back arrow. The title takes focus on open and draws no box. `tab` (v-model) is kept in the
 * address as `?tab=`. Close and Back go where the app's sheets go unless you listen for `@close` / `@back` yourself.
 */
const props = withDefaults(
  defineProps<{
    title: string;
    /** Data that says whose page this is: "dame_g · Lichess". */
    subtitle?: string;
    tabs?: SheetTab[];
    wide?: boolean;
    loading?: boolean;
    error?: string | null;
    /** Fills the nearest positioned parent at any window width instead of docking or covering the window: for a preview. */
    contained?: boolean;
    /** Keep `tab` in the address (`?tab=`); on by default. */
    sync?: boolean;
  }>(),
  { sync: true },
);

const emit = defineEmits<{ close: []; back: []; retry: [] }>();
defineSlots<{
  actions?(): unknown;
  lead?(): unknown;
  default?(props: { tab: string | undefined }): unknown;
  [slot: `tab-${string}`]: (() => unknown) | undefined;
}>();
const tab = defineModel<string>("tab");
const uid = useId();
const titleId = `${uid}-title`;
const nav = useSheetNav();
const own = getCurrentInstance()?.vnode.props;
const onClose = () => (own?.["onClose"] ? emit("close") : nav.close());
const onBack = () => (own?.["onBack"] ? emit("back") : nav.back());

const roomy = useMedia(up("lg"));
const docked = computed(() => props.contained || roomy.value);
const tabbed = computed(() => (props.tabs?.length ?? 0) > 1);
const current = computed(() => {
  const ids = props.tabs?.map((t) => t.id) ?? [];
  return tab.value && ids.includes(tab.value) ? tab.value : ids[0];
});

// The tab is part of the address, so a reload or a shared link lands on the same one.
const router = inject(routerKey, null);
const route = inject(routeLocationKey, null);
/** The address names a tab once the tabs exist (a sheet that is still loading has none yet): apply it, once. */
let applied = false;
/** The tab taken from the address that the owner's `v-model` has not handed back yet: the address is left alone until it does. */
let pending: string | null = null;
function fromAddress() {
  const asked = route?.query["tab"];
  if (applied || !props.sync || typeof asked !== "string" || !props.tabs?.some((t) => t.id === asked)) return;
  applied = true;
  if (asked !== current.value) pending = asked;
  tab.value = asked;
}
onMounted(fromAddress);
watch(() => props.tabs?.map((t) => t.id).join(), fromAddress, { flush: "sync" });
watch(current, () => {
  const id = current.value;
  if (pending !== null) {
    // The tab the address asked for is on its way in: do not write the one shown meanwhile over it.
    if (id !== pending) return;
    pending = null;
  }
  if (!props.sync || !router || !route || !tabbed.value || route.query["tab"] === id) return;
  void router.replace({ query: { ...route.query, tab: id } });
});

/** What opened the sheet: focus goes back there when the last sheet closes. */
const opener = captureOpener();
const root = useTemplateRef<HTMLElement>("root");
onBeforeUnmount(() => returnToOpener(root.value, opener, (run) => void nextTick(run)));

const heading = useTemplateRef<HTMLElement>("heading");
onMounted(() => void nextTick(() => heading.value?.focus({ preventScroll: true })));

function choose(id: string) {
  pending = null;
  tab.value = id;
}

const buttons = useTemplateRef<HTMLButtonElement[]>("tabButtons");
function onTabKey(e: KeyboardEvent, from: number) {
  const ids = props.tabs ?? [];
  let to: number;
  if (e.key === "ArrowRight") to = (from + 1) % ids.length;
  else if (e.key === "ArrowLeft") to = (from - 1 + ids.length) % ids.length;
  else if (e.key === "Home") to = 0;
  else if (e.key === "End") to = ids.length - 1;
  else return;
  e.preventDefault();
  const next = ids[to];
  if (!next) return;
  choose(next.id);
  void nextTick(() => buttons.value?.[to]?.focus());
}

/** A layer over the sheet (dialog, menu, popover, list) takes Escape before the sheet does. */
const LAYERS =
  '[role="dialog"][data-state="open"], [role="alertdialog"], [role="menu"], [role="listbox"], [data-e-popover]';
function onEscape(e: KeyboardEvent) {
  if (e.key !== "Escape" || e.defaultPrevented || document.querySelector(LAYERS)) return;
  onClose();
}
onMounted(() => document.addEventListener("keydown", onEscape));
onBeforeUnmount(() => document.removeEventListener("keydown", onEscape));
</script>

<template>
  <section
    ref="root"
    class="ui-sheet"
    :class="{ 'ui-sheet--wide': wide, 'ui-sheet--contained': contained }"
    role="dialog"
    :aria-modal="docked ? undefined : 'true'"
    :aria-labelledby="titleId"
    :aria-busy="loading || undefined"
  >
    <header class="ui-sheet-head">
      <button
        v-if="!docked || nav.canGoBack.value"
        type="button"
        class="ui-sheet-icon"
        aria-label="Back"
        @click="onBack"
      >
        <PhArrowLeft aria-hidden="true" />
      </button>
      <div class="ui-sheet-titles">
        <h1 :id="titleId" ref="heading" class="ui-sheet-title truncate" tabindex="-1" :title="title">{{ title }}</h1>
        <p v-if="subtitle" class="ui-sheet-subtitle truncate">{{ subtitle }}</p>
      </div>
      <div v-if="$slots['actions']" class="ui-sheet-actions"><slot name="actions" /></div>
      <button v-if="docked" type="button" class="ui-sheet-icon" aria-label="Close" @click="onClose">
        <PhX aria-hidden="true" />
      </button>
    </header>

    <div class="ui-sheet-scroll">
      <template v-if="loading">
        <div class="ui-sheet-lead" aria-hidden="true">
          <span class="skel ui-sheet-skel-hero" />
          <span class="skel ui-sheet-skel-hero" />
          <span class="skel ui-sheet-skel-hero" />
        </div>
        <div class="ui-sheet-tabs" aria-hidden="true"><span class="skel ui-sheet-skel-tab" /></div>
        <div class="ui-sheet-body" aria-hidden="true">
          <span class="skel ui-sheet-skel-line" />
          <span class="skel ui-sheet-skel-block" />
          <span class="skel ui-sheet-skel-line" />
        </div>
      </template>
      <UiEmpty v-else-if="error" tone="error" title="This did not load" :body="error">
        <template #actions>
          <EButton variant="outline" @click="emit('retry')">Try again</EButton>
        </template>
      </UiEmpty>
      <template v-else>
        <div v-if="$slots['lead']" class="ui-sheet-lead"><slot name="lead" /></div>
        <div v-if="tabbed" class="ui-sheet-tabs" role="tablist" :aria-label="`${title} sections`">
          <button
            v-for="(t, i) in tabs"
            :id="`${uid}-tab-${t.id}`"
            :key="t.id"
            ref="tabButtons"
            type="button"
            class="ui-sheet-tab"
            :class="{ 'is-on': t.id === current }"
            role="tab"
            :aria-selected="t.id === current"
            :aria-controls="`${uid}-panel-${t.id}`"
            :tabindex="t.id === current ? 0 : -1"
            @click="choose(t.id)"
            @keydown="onTabKey($event, i)"
          >
            {{ t.label }}<span v-if="t.count != null" class="ui-sheet-count num">{{ t.count }}</span>
          </button>
        </div>
        <div
          :id="current ? `${uid}-panel-${current}` : undefined"
          class="ui-sheet-body"
          :role="tabbed ? 'tabpanel' : undefined"
          :aria-labelledby="tabbed && current ? `${uid}-tab-${current}` : undefined"
        >
          <slot v-if="current && $slots[`tab-${current}`]" :name="`tab-${current}`" />
          <slot v-else :tab="current" />
        </div>
      </template>
    </div>
  </section>
</template>

<style scoped>
.ui-sheet {
  position: absolute;
  inset-block: 0;
  inset-inline-end: 0;
  z-index: var(--e-z-raised);
  display: flex;
  flex-direction: column;
  box-sizing: border-box;
  width: min(var(--w-sheet), 100%);
  min-height: 0;
  border-inline-start: var(--e-stroke-hair) solid var(--c-line);
  background: var(--c-pane);
  color: var(--c-ink);
}

/* It arrives from its edge; eos zeroes the duration under reduced motion, and so does this. */
.ui-sheet:not(.ui-sheet--contained) {
  animation: ui-sheet-in var(--m-move) var(--m-ease);
}

@keyframes ui-sheet-in {
  from {
    opacity: 0;
    transform: translateX(var(--sp-5));
  }
}

@media (prefers-reduced-motion: reduce) {
  .ui-sheet {
    animation: none;
  }
}

.ui-sheet--wide {
  width: min(var(--w-sheet-wide), 100%);
}

/* Below 75rem the sheet is the page: it covers the window, less what the app keeps visible (a header, a tab bar). */
@media (width < 75rem) {
  .ui-sheet,
  .ui-sheet--wide {
    position: fixed;
    inset: var(--sheet-inset-top) 0 var(--sheet-inset-bottom);
    z-index: var(--e-z-modal);
    width: auto;
    border-inline-start: 0;
  }
}

.ui-sheet.ui-sheet--contained {
  position: absolute;
  inset: 0;
  z-index: auto;
  width: auto;
  border-inline-start: 0;
}

.ui-sheet-head {
  display: flex;
  flex: none;
  align-items: center;
  gap: var(--sp-2);
  box-sizing: border-box;
  height: var(--h-header);
  padding-inline: var(--sp-3);
  border-block-end: var(--e-stroke-hair) solid var(--c-line);
}

.ui-sheet-titles {
  flex: 1 1 auto;
  min-width: 0;
}

.ui-sheet-title {
  margin: 0;
  font: var(--t-title);
}

/* Focus moves here when the sheet opens so a screen reader starts at its name; nothing is drawn around it. */
.ui-sheet-title:focus {
  outline: none;
}

.ui-sheet-subtitle {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.ui-sheet-actions {
  display: flex;
  flex: none;
  align-items: center;
  gap: var(--sp-2);
}

.ui-sheet-icon {
  display: inline-grid;
  flex: none;
  place-items: center;
  width: var(--h-control);
  height: var(--h-control);
  padding: 0;
  border: 0;
  border-radius: var(--r-1);
  background: transparent;
  color: var(--c-ink-2);
  font-size: var(--i-lg);
  cursor: pointer;
}

.ui-sheet-icon:hover {
  background: var(--c-raise);
  color: var(--c-ink);
}

.ui-sheet-icon:focus-visible {
  outline: var(--focus-ring);
  outline-offset: calc(-1 * var(--e-stroke-focus));
}

.ui-sheet-scroll {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  overscroll-behavior: contain;
}

.ui-sheet-lead {
  display: grid;
  gap: var(--sp-3);
  padding: var(--sp-3);
}

.ui-sheet-tabs {
  position: sticky;
  inset-block-start: 0;
  z-index: var(--e-z-sticky);
  display: flex;
  gap: var(--sp-1);
  padding-inline: var(--sp-3);
  overflow-x: auto;
  border-block-end: var(--e-stroke-hair) solid var(--c-line);
  background: var(--c-pane);
  scrollbar-width: none;
}

.ui-sheet-tab {
  position: relative;
  display: inline-flex;
  flex: none;
  gap: var(--sp-1);
  align-items: center;
  min-height: var(--h-tab);
  padding: 0 var(--sp-2);
  border: 0;
  background: transparent;
  color: var(--c-ink-2);
  font: 500 var(--e-text-md) / 1 var(--e-font-sans);
  cursor: pointer;
}

.ui-sheet-tab:hover {
  color: var(--c-ink);
}

.ui-sheet-tab.is-on {
  color: var(--c-ink);
  font-weight: 600;
}

.ui-sheet-tab.is-on::after {
  content: "";
  position: absolute;
  inset: auto var(--sp-2) 0;
  height: var(--e-stroke-focus);
  background: var(--c-act);
}

.ui-sheet-tab:focus-visible {
  outline: var(--focus-ring);
  outline-offset: calc(-1 * var(--e-stroke-focus));
}

.ui-sheet-count {
  color: var(--c-ink-2);
  font: var(--t-fig);
  font-size: var(--e-text-xs);
}

.ui-sheet-body {
  display: grid;
  align-content: start;
  gap: var(--sp-4);
  padding: var(--sp-3);
}

@media (width >= 56rem) {
  .ui-sheet-lead,
  .ui-sheet-body {
    padding: var(--sp-4);
  }

  .ui-sheet-tabs {
    padding-inline: var(--sp-4);
  }
}

.ui-sheet-skel-hero {
  height: 2.5rem;
}

.ui-sheet-skel-tab {
  width: 14rem;
  height: var(--h-tab);
}

.ui-sheet-skel-line {
  height: 1rem;
  max-width: 60%;
}

.ui-sheet-skel-block {
  height: 8rem;
}
</style>
