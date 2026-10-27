<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ECheckboxGroup } from "@eos-ui/vue/components/ECheckboxGroup";
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { EDisclosure } from "@eos-ui/vue/components/EDisclosure";
import { EFieldset } from "@eos-ui/vue/components/EFieldset";
import { EForm } from "@eos-ui/vue/components/EForm";
import { ENumberField } from "@eos-ui/vue/components/ENumberField";
import { ESelect } from "@eos-ui/vue/components/ESelect";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import type { BotSettings, Speed } from "../api/types";
import { speedName } from "../format";
import { useConnections } from "../stores/connections";
import { useVersions } from "../stores/versions";

/**
 * The bot's rules: which version plays, which speeds and clocks and challengers it takes, how many games at once, and
 * the clocks it looks for games at. Clocks are written in minutes here and kept in seconds, as the settings are. It is
 * a form: Enter saves, and what stops a save is said beside the field that needs the change.
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
  seekText.value = props.settings.seek_clocks.join(", ");
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

const version = computed({
  get: () => draft.value.version ?? "champion",
  set: (v: string | undefined) => (draft.value.version = v || "champion"),
});

/** Minutes in the field, seconds in the settings. */
const minutes = (key: "min_initial" | "max_initial") =>
  computed({
    get: () => draft.value[key] / 60,
    set: (v: number | null) => (draft.value[key] = Math.round((v ?? 0) * 60)),
  });
const minInitial = minutes("min_initial");
const maxInitial = minutes("max_initial");

const clocks = computed(() =>
  seekText.value
    .split(/[,\s]+/)
    .map((c) => c.trim())
    .filter(Boolean),
);

const speedsError = computed(() => (draft.value.speeds.length ? undefined : "Pick at least one speed."));
const longestError = computed(() =>
  draft.value.max_initial < draft.value.min_initial ? "Not shorter than the shortest." : undefined,
);
const largestError = computed(() =>
  draft.value.max_increment < draft.value.min_increment ? "Not less than the least." : undefined,
);
const clocksError = computed(() =>
  clocks.value.some((c) => !/^\d+(\.\d+)?\+\d+$/.test(c))
    ? "Write each clock as minutes+seconds, like 3+2."
    : undefined,
);
const blocked = computed(() => !!(speedsError.value || longestError.value || largestError.value || clocksError.value));

async function save() {
  if (blocked.value) return;
  busy.value = true;
  try {
    await connections.saveBotSettings({ ...draft.value, seek_clocks: clocks.value });
    open.value = false;
    toast.show({ title: "The bot's rules are saved", tone: "success" });
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
  <EDialog
    v-model:open="open"
    title="The bot's rules"
    description="What Dameless plays on Lichess, and which challenges it takes."
    size="md"
    initial-focus="#bot-rules button, #bot-rules input"
  >
    <EForm id="bot-rules" @submit.prevent="save">
      <ESelect v-model="version" label="Version that plays" :options="versionOptions" />
      <ECheckboxGroup
        v-model="draft.speeds"
        label="Speeds"
        orientation="horizontal"
        :options="speedOptions"
        :error="speedsError"
        hint="Correspondence and UltraBullet are never played."
      />
      <EFieldset legend="Takes challenges" variant="plain">
        <div class="pair">
          <ESwitch v-model="draft.accept_rated" label="Rated" />
          <ESwitch v-model="draft.accept_casual" label="Casual" />
          <ESwitch v-model="draft.accept_people" label="From people" />
          <ESwitch v-model="draft.accept_bots" label="From bots" />
        </div>
      </EFieldset>
      <EFieldset legend="Game length" variant="plain">
        <div class="pair">
          <ENumberField v-model="minInitial" label="Shortest" suffix="min" :min="0" :max="180" :step="0.5" />
          <ENumberField
            v-model="maxInitial"
            label="Longest"
            suffix="min"
            :min="0"
            :max="180"
            :step="1"
            :error="longestError"
          />
          <ENumberField v-model="draft.min_increment" label="Least added time" suffix="s" :min="0" :max="180" />
          <ENumberField
            v-model="draft.max_increment"
            label="Most added time"
            suffix="s"
            :min="0"
            :max="180"
            :error="largestError"
          />
        </div>
      </EFieldset>
      <div class="pair">
        <ENumberField v-model="draft.max_games" label="Games at once" :min="1" :max="16" />
      </div>
      <ETextField
        v-model="seekText"
        label="Clocks to look for games at"
        :error="clocksError"
        hint="Minutes plus seconds, separated by commas: 3+0, 3+2, 5+3."
        autocomplete="off"
      />
      <EDisclosure title="Engine">
        <div class="pair">
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
      </EDisclosure>
    </EForm>
    <template #actions>
      <EButton type="submit" form="bot-rules" :loading="busy" :disabled="blocked">Save rules</EButton>
      <EButton variant="outline" @click="open = false">Cancel</EButton>
    </template>
  </EDialog>
</template>

<style scoped>
/* Grouped fields sit two to a row; the form itself puts every group on its own row. */
.pair {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--e-gap-snug) var(--e-gap-base);
}
</style>
