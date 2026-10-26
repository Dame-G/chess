<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { PhX } from "@phosphor-icons/vue";
import { ECheckboxGroup } from "@eos-ui/vue/components/ECheckboxGroup";
import { ECombobox } from "@eos-ui/vue/components/ECombobox";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDisclosure } from "@eos-ui/vue/components/EDisclosure";
import { EForm } from "@eos-ui/vue/components/EForm";
import { EGrid } from "@eos-ui/vue/components/EGrid";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ENumberField } from "@eos-ui/vue/components/ENumberField";
import { ERadioGroup } from "@eos-ui/vue/components/ERadioGroup";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import { api } from "../../api/runner";
import { unwrap, useBuilds, useStatus, useTunables, type Branch, type Param } from "../../api/queries";
import type { components } from "../../api/schema";
import BranchPicker from "../BranchPicker.vue";
import { pathOf } from "../improve/improve";
import { controls, gamesIn } from "./tune";

type NewTune = components["schemas"]["NewTune"];
type ParamInput = components["schemas"]["ParamInput"];

const emit = defineEmits<{ done: [] }>();
/** The form for tuning a branch. The dialog around it owns the buttons. */
const busy = defineModel<boolean>("busy", { default: false });
const valid = defineModel<boolean>("valid", { default: false });
const router = useRouter();
const { state: builds } = useBuilds();
const { state: status } = useStatus();

const DEFAULT_REF = "spsa/tunables";
const ref_ = ref(DEFAULT_REF);
const name = ref("");
const iterations = ref<number | null>(1000);
const pairs = ref<number | null>(8);
const control = ref<(typeof controls)[number]["value"]>("stc");
const nodes = ref<number | null>(25_000);
const concurrency = ref<number | null>(null);
const alpha = ref<number | null>(null);
const gamma = ref<number | null>(null);
const aRatio = ref<number | null>(null);
const hash = ref<number | null>(null);
const error = ref("");

/** Whether a branch is likely to declare constants to tune: a tunables branch, a tune applied to one, or one built for it. */
const tunable = (b: Branch) =>
  /(^|\/)spsa/.test(b.name) ||
  b.name.startsWith("tune/") ||
  !!builds.value.data?.some((x) => x.ref === b.name && /spsa/.test(x.features));

// ---- Which parameters: the engine declares them, once a build with the spsa feature exists
/** The newest build of the ref that has its tunables as options. */
const spsaBuild = computed(() =>
  builds.value.data?.find((b) => b.ref === ref_.value.trim() && /spsa/.test(b.features)),
);
const { state: tunables } = useTunables(() => spsaBuild.value?.id ?? 0);
const declared = computed<Param[]>(() => (spsaBuild.value ? (tunables.value.data ?? []) : []));

const scope = ref<"all" | "some">("all");
const chosen = ref<string[]>([]);
const names = computed(() => declared.value.map((p) => ({ value: p.name, label: p.name })));
watch(declared, () => (chosen.value = chosen.value.filter((n) => declared.value.some((p) => p.name === n))));

// ---- Overrides, one row per parameter the user wants to set by hand
interface Override {
  name: string;
  start: number | null;
  min: number | null;
  max: number | null;
  c_end: number | null;
  r_end: number | null;
}
const overrides = ref<Override[]>([]);
const adding = ref("");
const addable = computed(() =>
  declared.value
    .filter((p) => !overrides.value.some((o) => o.name === p.name))
    .filter((p) => scope.value === "all" || chosen.value.includes(p.name))
    .map((p) => ({ value: p.name, label: p.name, description: `${p.min} to ${p.max}, starts at ${p.start}` })),
);
watch(adding, (n) => {
  if (!n) return;
  overrides.value.push({ name: n, start: null, min: null, max: null, c_end: null, r_end: null });
  adding.value = "";
});
const declaredOf = (n: string) => declared.value.find((p) => p.name === n);
const fields = ["start", "min", "max", "c_end", "r_end"] as const;
const fieldLabel = { start: "Start", min: "Lowest", max: "Highest", c_end: "Last nudge", r_end: "Last step" } as const;

// ---- What it will do
const total = computed(() => gamesIn(iterations.value ?? 0, pairs.value ?? 0));
const count = computed(() => (scope.value === "some" ? chosen.value.length : declared.value.length || undefined));
const hasThreads = computed(() => status.value.data?.cpu.threads);

function body(): NewTune {
  const out: NewTune = { build: ref_.value.trim() };
  if (name.value.trim()) out.name = name.value.trim();
  if (iterations.value) out.iterations = iterations.value;
  if (pairs.value) out.pairs_per_iteration = pairs.value;
  if (control.value === "nodes") {
    out.preset = "nodes";
    if (nodes.value) out.nodes = nodes.value;
  } else out.preset = control.value;
  if (concurrency.value) out.concurrency = concurrency.value;
  if (alpha.value != null) out.alpha = alpha.value;
  if (gamma.value != null) out.gamma = gamma.value;
  if (aRatio.value != null) out.a_ratio = aRatio.value;
  if (hash.value) out.hash = hash.value;
  const picked = scope.value === "some" ? chosen.value : declared.value.map((p) => p.name);
  const set = overrides.value.filter((o) => picked.includes(o.name));
  if (scope.value === "some" || set.length) {
    out.params = picked.map((n) => {
      const o = set.find((x) => x.name === n);
      const p: ParamInput = { name: n };
      for (const f of fields) if (o && o[f] != null) p[f] = o[f];
      return p;
    });
  }
  return out;
}

async function submit() {
  busy.value = true;
  error.value = "";
  try {
    const t = await unwrap(api.POST("/api/tunes", { body: body() }));
    name.value = "";
    overrides.value = [];
    emit("done");
    await router.push(pathOf("tune", t.id));
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}

const complete = computed(
  () =>
    !!ref_.value.trim() &&
    (iterations.value ?? 0) > 0 &&
    (pairs.value ?? 0) > 0 &&
    (scope.value === "all" || chosen.value.length > 0),
);
watch(complete, (ok) => (valid.value = ok), { immediate: true });
</script>

<template>
  <EForm id="new-tune" :submitting="busy" @submit="submit">
    <EGrid min="14rem" gap="base">
      <BranchPicker
        v-model="ref_"
        label="Branch to tune"
        :filter="tunable"
        allow-ref
        required
        hint="It must declare constants to tune, as spsa/tunables does. The tune builds its own copy."
      />
      <ETextField v-model="name" label="Name" :placeholder="ref_" optional hint="Left empty, it uses the branch." />
    </EGrid>

    <div class="scope">
      <ESegmented
        v-model="scope"
        label="Which constants"
        :options="[
          { value: 'all', label: declared.length ? `All ${declared.length}` : 'All of them' },
          { value: 'some', label: 'Choose', disabled: !declared.length },
        ]"
      />
      <p class="note">
        <template v-if="declared.length">
          {{
            scope === "all"
              ? "Every constant the engine declares will be tuned."
              : `${chosen.length} of ${declared.length} chosen.`
          }}
        </template>
        <template v-else>
          Every constant the engine declares is tuned. They are read from the engine when it builds, so the list is here
          once this branch has been tuned or built for tuning.
        </template>
      </p>
      <ECheckboxGroup
        v-if="scope === 'some'"
        v-model="chosen"
        class="names"
        label="Constants to tune"
        orientation="horizontal"
        :options="names"
      />
    </div>

    <EGrid min="10rem" gap="base">
      <ENumberField v-model="iterations" label="Iterations" :min="1" :step="100" hint="Each takes a step." />
      <ENumberField v-model="pairs" label="Game pairs each" :min="1" hint="More pairs, less noise per step." />
      <ENumberField
        v-model="concurrency"
        label="Games at once"
        :min="1"
        optional
        :hint="
          hasThreads
            ? `Left empty: this machine's ${hasThreads} threads, less two.`
            : 'Left empty: the machine’s threads, less two.'
        "
      />
    </EGrid>

    <ERadioGroup
      v-model="control"
      label="How long are the games?"
      :options="controls.map((c) => ({ value: c.value, label: c.label, description: c.detail }))"
    />
    <ENumberField
      v-if="control === 'nodes'"
      v-model="nodes"
      label="Nodes per move"
      :min="1000"
      :step="5000"
      hint="No clock, so speed does not count."
    />

    <p class="plan">
      {{ (iterations ?? 0).toLocaleString("en") }} iterations of {{ pairs ?? 0 }} pairs is
      <b>{{ total.toLocaleString("en") }} games</b><template v-if="count"> for {{ count }} constants</template>. It
      queues behind any test already running.
    </p>

    <EDisclosure title="Advanced">
      <div class="advanced">
        <EGrid min="10rem" gap="base">
          <ENumberField
            v-model="alpha"
            label="Step decay (alpha)"
            :min="0.1"
            :step="0.01"
            optional
            hint="Default 0.602."
          />
          <ENumberField
            v-model="gamma"
            label="Nudge decay (gamma)"
            :min="0.01"
            :step="0.01"
            optional
            hint="Default 0.101."
          />
          <ENumberField
            v-model="aRatio"
            label="Stability (A ratio)"
            :min="0"
            :step="0.05"
            optional
            hint="Share of the iterations. Default 0.1."
          />
          <ENumberField v-model="hash" label="Hash (MB)" :min="1" optional hint="Left empty, the time control's." />
        </EGrid>

        <div class="over">
          <ECombobox
            v-model="adding"
            label="Set a constant by hand"
            :options="addable"
            :disabled="!declared.length"
            :hint="
              declared.length
                ? 'Start, range and step sizes. Anything left empty keeps what the engine declares.'
                : 'Available once the engine has been built for tuning.'
            "
            placeholder="Pick a constant"
          />
          <div v-for="(o, i) in overrides" :key="o.name" class="override">
            <div class="override-head">
              <b class="num">{{ o.name }}</b>
              <EButton
                variant="quiet"
                size="sm"
                :icon="PhX"
                :aria-label="`Remove ${o.name}`"
                @click="overrides.splice(i, 1)"
              />
            </div>
            <EGrid min="7rem" gap="snug">
              <ENumberField
                v-for="f in fields"
                :key="f"
                v-model="o[f]"
                size="sm"
                :label="fieldLabel[f]"
                optional
                :step="f === 'r_end' ? 0.001 : 1"
                :hint="`Declared ${declaredOf(o.name)?.[f] ?? ''}`"
              />
            </EGrid>
          </div>
        </div>
      </div>
    </EDisclosure>

    <ENotice v-if="error" tone="danger" title="Could not start it" :role="false">{{ error }}</ENotice>
  </EForm>
</template>

<style scoped>
.scope,
.advanced,
.over {
  display: grid;
  gap: var(--e-gap-base);
  min-width: 0;
}

.names {
  max-height: 12rem;
  padding: var(--e-gap-base);
  border: var(--e-stroke-hair) solid var(--e-border);
  border-radius: var(--e-radius-sm);
  overflow: auto;
}

.note,
.plan {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.plan b {
  color: var(--e-ink);
}

.override {
  display: grid;
  gap: var(--e-gap-snug);
  padding: var(--e-gap-base);
  border: var(--e-stroke-hair) solid var(--e-border);
  border-radius: var(--e-radius-sm);
}

.override-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
</style>
