<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { EForm } from "@eos-ui/vue/components/EForm";
import { EGrid } from "@eos-ui/vue/components/EGrid";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ERadioGroup } from "@eos-ui/vue/components/ERadioGroup";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import { ETag } from "@eos-ui/vue/components/ETag";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import { ETextarea } from "@eos-ui/vue/components/ETextarea";
import { api } from "../../api/runner";
import { unwrap, useBranches } from "../../api/queries";
import { kinds, stageLabel } from "../../format";
import BranchPicker from "../BranchPicker.vue";
import { pathOf } from "./improve";

const props = defineProps<{ branch?: string }>();
const emit = defineEmits<{ done: [] }>();
/** The form for testing one branch against another. The dialog around it owns the buttons. */
const valid = defineModel<boolean>("valid", { default: false });
const busy = defineModel<boolean>("busy", { default: false });
const router = useRouter();
const { state: branches } = useBranches();

const dev = ref(props.branch ?? "");
watch(
  () => props.branch,
  (b) => {
    if (b) dev.value = b;
  },
);
const base = ref("main");
const kind = ref<keyof typeof kinds>("gainer");
const name = ref("");
const hypothesis = ref("");
const ltc = ref(false);
const error = ref("");

// The newest branch of the fork that is not the base is the likely one to test.
watch(
  () => branches.value.data,
  (list) => {
    if (!dev.value)
      dev.value = list?.find((b) => b.name !== base.value && !b.name.startsWith("upstream/") && !b.hidden)?.name ?? "";
  },
  { immediate: true },
);
const subject = computed(() => branches.value.data?.find((b) => b.name === dev.value)?.subject);

watch([dev, base], () => (valid.value = !!dev.value.trim() && !!base.value.trim()), { immediate: true });

/** The stages it will go through, in words. */
const plan = computed(() =>
  (kind.value === "nonfunctional" ? ["bench", "speed"] : ["bench", "screen", "stc", ...(ltc.value ? ["ltc"] : [])]).map(
    stageLabel,
  ),
);

async function submit() {
  busy.value = true;
  error.value = "";
  try {
    const e = await unwrap(
      api.POST("/api/experiments", {
        body: {
          dev: dev.value.trim(),
          base: base.value.trim(),
          kind: kind.value,
          name: name.value.trim() || subject.value || dev.value,
          hypothesis: hypothesis.value,
          ltc: kind.value !== "nonfunctional" && ltc.value,
        },
      }),
    );
    dev.value = "";
    name.value = "";
    hypothesis.value = "";
    emit("done");
    await router.push(pathOf("test", e.id));
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <EForm id="new-test" :submitting="busy" @submit="submit">
    <EGrid min="14rem" gap="base">
      <BranchPicker
        v-model="dev"
        label="Branch to test"
        :exclude="[base]"
        allow-ref
        required
        hint="The version with your change."
      />
      <BranchPicker v-model="base" label="Compare with" :exclude="[dev]" allow-ref required hint="Usually main." />
    </EGrid>
    <ERadioGroup
      v-model="kind"
      label="What should the change do?"
      :options="Object.entries(kinds).map(([value, k]) => ({ value, label: k.label, description: k.detail }))"
    />
    <ETextField
      v-model="name"
      label="Name"
      :placeholder="subject"
      optional
      hint="Left empty, it uses the commit's title."
    />
    <ETextarea
      v-model="hypothesis"
      label="Why should it work?"
      :rows="2"
      optional
      placeholder="Pruning more at shallow depth saves time for the moves that matter."
    />
    <ESwitch
      v-if="kind !== 'nonfunctional'"
      v-model="ltc"
      label="Confirm with long games too"
      hint="Adds 40-second games after the short ones: a few more hours."
    />
    <div class="plan">
      <span class="plan-label">It will go through</span>
      <template v-for="(p, i) in plan" :key="p">
        <span v-if="i" class="plan-arrow" aria-hidden="true">→</span>
        <ETag size="sm" :label="p" />
      </template>
    </div>
    <ENotice v-if="error" tone="danger" title="Could not start it" :role="false">{{ error }}</ENotice>
  </EForm>
</template>

<style scoped>
.plan {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--e-gap-tight);
  color: var(--e-muted);
  font: var(--e-type-small);
}

.plan-label {
  margin-inline-end: var(--e-gap-tight);
}
</style>
