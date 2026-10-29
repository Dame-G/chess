<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useToast } from "@eos-ui/vue";
import { ECheckbox } from "@eos-ui/vue/components/ECheckbox";
import { ECheckboxGroup } from "@eos-ui/vue/components/ECheckboxGroup";
import { EDisclosure } from "@eos-ui/vue/components/EDisclosure";
import { ENumberField } from "@eos-ui/vue/components/ENumberField";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import type { BotSettings, Speed } from "../api/types";
import { speedName } from "../format";
import { useConnections } from "../stores/connections";
import { useVersions } from "../stores/versions";
import UiBadge from "../ui/UiBadge.vue";
import UiDialog from "../ui/UiDialog.vue";
import UiField from "../ui/UiField.vue";
import UiSelect from "../ui/UiSelect.vue";

/**
 * Bot rules: which version plays, which speeds and clocks and challengers it takes, how many games at once, and the
 * clocks it looks for games at. Clocks are written in minutes here and kept in seconds, as the settings are. It is a
 * form: Enter saves, and what stops a save is said at the field that needs the change.
 */
const props = defineProps<{ settings: BotSettings }>();
const open = defineModel<boolean>("open", { required: true });

const connections = useConnections();
const versions = useVersions();
const toast = useToast();

const draft = ref<BotSettings>({ ...props.settings });
const seekText = ref("");
const busy = ref(false);

watch(open, (isOpen) => {
  if (!isOpen) return;
  draft.value = { ...props.settings, speeds: [...props.settings.speeds] };
  seekText.value = props.settings.seek_clocks.join(" ");
  versionError.value = undefined;
});

const speedOptions: { value: Speed; label: string }[] = (["bullet", "blitz", "rapid", "classical"] as const).map(
  (s) => ({
    value: s,
    label: speedName[s],
  }),
);
const versionOptions = computed(() => [
  { value: "champion", label: "Champion, as of each game" },
  ...[...versions.line].reverse().map((v) => ({ value: v.name, label: v.name })),
]);

/** The version shown is the one saved or one just chosen: with none saved, the select is empty. */
const version = computed({
  get: () => draft.value.version ?? undefined,
  set: (v: string | undefined) => (draft.value.version = v || null),
});
const versionError = ref<string>();
watch(version, () => (versionError.value = undefined));

/** Minutes in the field, seconds in the settings. */
const minutes = (key: "min_initial" | "max_initial") =>
  computed({
    get: () => draft.value[key] / 60,
    set: (v: number | null) => (draft.value[key] = Math.round((v ?? 0) * 60)),
  });
const minInitial = minutes("min_initial");
const maxInitial = minutes("max_initial");

const CLOCK = /^\d+(\.\d+)?\+\d+$/;
const clocks = computed(() =>
  seekText.value
    .split(/[,\s]+/)
    .map((c) => c.trim())
    .filter(Boolean),
);

const speedsError = computed(() => (draft.value.speeds.length ? undefined : "Pick at least one speed."));
const longestError = computed(() =>
  draft.value.max_initial < draft.value.min_initial ? "The longest clock is shorter than the shortest." : undefined,
);
const largestError = computed(() =>
  draft.value.max_increment < draft.value.min_increment ? "The most added time is less than the least." : undefined,
);
const clocksError = computed(() =>
  clocks.value.some((c) => !CLOCK.test(c)) ? "Write each clock as minutes+seconds, like 3+2." : undefined,
);
const blocked = computed(() => !!(speedsError.value || longestError.value || largestError.value || clocksError.value));

async function save() {
  if (!draft.value.version) {
    versionError.value = "Choose the version that plays.";
    return;
  }
  if (blocked.value) return;
  busy.value = true;
  try {
    await connections.saveBotSettings({ ...draft.value, seek_clocks: clocks.value });
    open.value = false;
    toast.show({ title: "Bot rules saved", tone: "success" });
  } catch (e) {
    toast.show({
      title: "The rules were not saved",
      message: e instanceof Error ? e.message : String(e),
      tone: "warning",
    });
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <UiDialog
    v-model:open="open"
    title="Bot rules"
    size="lg"
    confirm-label="Save rules"
    :busy="busy"
    :confirm-disabled="blocked"
    @confirm="save"
  >
    <div class="stack">
      <UiSelect
        v-model="version"
        label="Version that plays"
        placeholder="Choose a version"
        :options="versionOptions"
        :error="versionError"
      />

      <UiField group label="Takes challenges">
        <div class="checks">
          <ECheckbox v-model="draft.accept_rated" label="Rated" />
          <ECheckbox v-model="draft.accept_casual" label="Casual" />
          <ECheckbox v-model="draft.accept_people" label="From people" />
          <ECheckbox v-model="draft.accept_bots" label="From bots" />
        </div>
      </UiField>

      <UiField group label="Speeds" :error="speedsError" hint="Correspondence and UltraBullet are never played.">
        <ECheckboxGroup v-model="draft.speeds" label="Speeds" hide-label :columns="2" :options="speedOptions" />
      </UiField>

      <UiField group label="Clock" :error="longestError">
        <div class="range">
          <ENumberField
            v-model="minInitial"
            label="Shortest clock"
            hide-label
            suffix="min"
            :min="0"
            :max="180"
            :step="0.5"
          />
          <span class="to">to</span>
          <ENumberField v-model="maxInitial" label="Longest clock" hide-label suffix="min" :min="0" :max="180" />
        </div>
      </UiField>

      <UiField group label="Added time each move" :error="largestError">
        <div class="range">
          <ENumberField
            v-model="draft.min_increment"
            label="Least added time"
            hide-label
            suffix="s"
            :min="0"
            :max="180"
          />
          <span class="to">to</span>
          <ENumberField
            v-model="draft.max_increment"
            label="Most added time"
            hide-label
            suffix="s"
            :min="0"
            :max="180"
          />
        </div>
      </UiField>

      <UiField v-slot="{ id }" label="Games at once">
        <ENumberField
          :id="id"
          v-model="draft.max_games"
          label="Games at once"
          hide-label
          :min="1"
          :max="16"
          class="one"
        />
      </UiField>

      <UiField
        v-slot="{ id, describedby, invalid }"
        label="Clocks to look for games at"
        :error="clocksError"
        hint="Minutes plus seconds, separated by spaces: 3+0 3+2 5+3."
      >
        <ETextField
          :id="id"
          v-model="seekText"
          label="Clocks to look for games at"
          hide-label
          autocomplete="off"
          :aria-describedby="describedby"
          :aria-invalid="invalid || undefined"
        />
        <span v-if="clocks.length" class="chips" aria-hidden="true">
          <UiBadge v-for="c in clocks" :key="c" :tone="CLOCK.test(c) ? 'neutral' : 'loss'">{{ c }}</UiBadge>
        </span>
      </UiField>

      <EDisclosure title="Engine" title-style="text">
        <div class="stack engine">
          <div class="range">
            <ENumberField v-model="draft.hash" label="Hash" suffix="MiB" :min="1" :max="4096" />
            <ENumberField v-model="draft.threads" label="Threads" :min="1" :max="64" />
            <ENumberField
              v-model="draft.move_overhead"
              label="Move overhead"
              suffix="ms"
              :min="0"
              :max="2000"
              :step="50"
            />
          </div>
          <ESwitch
            v-model="draft.ponder"
            label="Think on the opponent's time"
            description="Searches the reply it expects while the opponent thinks. Needs a version that can ponder."
          />
        </div>
      </EDisclosure>
    </div>
  </UiDialog>
</template>

<style scoped>
.checks {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 0 var(--sp-3);
}

/* Two numbers that make a range sit on one line, each as wide as a number needs. */
.range {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--sp-2);
}

.range > :is(.e-number-field, div) {
  inline-size: var(--w-field-sm);
}

.one {
  inline-size: var(--w-field-sm);
}

.to {
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.chips {
  display: flex;
  flex-wrap: wrap;
  gap: var(--sp-1);
  margin-block-start: var(--sp-2);
}

.engine {
  padding-block-start: var(--sp-3);
}

@media (width < 30rem) {
  .checks {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
