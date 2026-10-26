<script setup lang="ts">
import { computed, ref } from "vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ECombobox } from "@eos-ui/vue/components/ECombobox";
import { useBranches } from "../api/queries";
import type { Branch } from "../api/queries";
import { shortAgo } from "./lab/lab";

/**
 * Chooses a branch of the engine: the fork's own first, then upstream's, newest first inside each. Type to search the
 * names and commit titles. A commit or tag can be typed instead when `allowRef` is on.
 */
const props = withDefaults(
  defineProps<{
    /** What the field is called. */
    label: string;
    /** Branches to leave out, such as the one already chosen on the other side. */
    exclude?: readonly string[];
    /** Keeps the branches it returns true for. */
    filter?: (branch: Branch) => boolean;
    /** A short note shown after a branch's commit title, such as "has tunables". */
    note?: (branch: Branch) => string | undefined;
    /** Offers "Use a commit or tag…", which turns the field into free text. */
    allowRef?: boolean;
    /** Lists branches that were hidden from the Versions page too. */
    showHidden?: boolean;
    hint?: string;
    placeholder?: string;
    required?: boolean;
    disabled?: boolean;
    clearable?: boolean;
    size?: "sm" | "md" | "lg";
  }>(),
  { exclude: () => [], filter: undefined, note: undefined, placeholder: "Search branches", size: "md" },
);
const emit = defineEmits<{
  /** The branch that was chosen (not when a commit or tag was typed). */
  pick: [branch: Branch];
}>();
/** The branch name, or the commit or tag that was typed. */
const model = defineModel<string>({ default: "" });
const { state } = useBranches();
const listed = computed<Branch[]>(() => state.value.data ?? []);
const upstream = (b: Branch) => b.name.startsWith("upstream/");

const kept = computed(() =>
  listed.value
    .filter((b) => (props.showHidden || !b.hidden) && !props.exclude.includes(b.name) && (props.filter?.(b) ?? true))
    .toSorted((a, b) => Number(upstream(a)) - Number(upstream(b)) || b.date.localeCompare(a.date)),
);
const known = computed(() => listed.value.some((b) => b.name === model.value));

// A value that is not a branch (a tag, a default the list lacks) stays visible as its own choice.
const options = computed(() => {
  const out = kept.value.map((b) => ({
    value: b.name,
    label: b.name,
    description: [b.subject, shortAgo(b.date), props.note?.(b)].filter(Boolean).join(" · "),
    group: upstream(b) ? "Upstream" : "This fork",
  }));
  if (model.value && !kept.value.some((b) => b.name === model.value) && state.value.status !== "pending")
    out.unshift({ value: model.value, label: model.value, description: known.value ? "" : "commit or tag", group: "" });
  return out;
});
// Groups are headings only when both kinds are listed.
const grouped = computed(() => {
  const mixed = new Set(options.value.map((o) => o.group)).size > 1;
  return options.value.map((o) => (mixed && o.group ? o : { ...o, group: undefined }));
});

const typing = ref(false);
const freeform = computed(
  () => props.allowRef && (typing.value || (!!model.value && !known.value && state.value.status === "success")),
);

function toggle() {
  if (freeform.value) {
    typing.value = false;
    if (!known.value) model.value = "";
  } else typing.value = true;
}

function change(value: string | undefined) {
  model.value = value ?? "";
  const b = listed.value.find((x) => x.name === model.value);
  if (b) emit("pick", b);
}
</script>

<template>
  <div class="picker">
    <ECombobox
      :model-value="model"
      :label="label"
      :options="grouped"
      :freeform="freeform"
      :loading="state.status === 'pending'"
      :placeholder="freeform ? 'A branch, commit or tag' : placeholder"
      :hint="hint"
      :required="required"
      :disabled="disabled"
      :clearable="clearable"
      :size="size"
      empty-text="No such branch"
      @update:model-value="change"
    />
    <EButton v-if="allowRef" class="ref" variant="quiet" size="sm" @click="toggle">
      {{ freeform ? "Choose from the branches" : "Use a commit or tag…" }}
    </EButton>
  </div>
</template>

<style scoped>
.picker {
  display: grid;
  gap: var(--e-gap-tight);
  justify-items: start;
  min-width: 0;
}

.picker > :first-child {
  justify-self: stretch;
}
</style>
