<script setup lang="ts">
import { computed, ref } from "vue";
import { useQueryCache } from "@pinia/colada";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDisclosure } from "@eos-ui/vue/components/EDisclosure";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ENumberField } from "@eos-ui/vue/components/ENumberField";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESpinner } from "@eos-ui/vue/components/ESpinner";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import { api } from "../../api/runner";
import { unwrap } from "../../api/queries";
import { clockLabel, meClockOptions, parseClock, speedName, speedOf, type MeStatus } from "../../lichess";
import { stored } from "../../stored";
import Term from "../Term.vue";
import { colours } from "./me";

const props = defineProps<{ me: MeStatus }>();

const cache = useQueryCache();
const clocks = meClockOptions("seek");
const clock = stored("lichess.me.seekClock", "10+5");
const rated = stored("lichess.me.rated", true);
const color = stored("lichess.me.color", "random");
const low = ref<number | null>(null);
const high = ref<number | null>(null);

const seeking = computed(() => props.me.seek.seeking ?? undefined);
const busy = ref(false);
const error = ref("");

const parsed = computed(() => parseClock(clock.value));
const speed = computed(() => speedName(speedOf(parsed.value.initial, parsed.value.increment)));
const rangeProblem = computed(() =>
  low.value !== null && high.value !== null && low.value > high.value ? "The lowest rating is above the highest." : "",
);
const range = computed(() =>
  low.value !== null && high.value !== null && !rangeProblem.value ? `${low.value}-${high.value}` : null,
);

async function run(call: () => Promise<MeStatus>) {
  busy.value = true;
  error.value = "";
  try {
    cache.setQueryData<MeStatus>(["lichess-me"], await call());
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
const find = () =>
  run(() =>
    unwrap(
      api.POST("/api/lichess/me/seek", {
        body: {
          clock_seconds: parsed.value.initial,
          increment: parsed.value.increment,
          rated: rated.value,
          color: color.value as "random" | "white" | "black",
          rating_range: range.value,
        },
      }),
    ),
  );
const cancel = () => run(() => unwrap(api.DELETE("/api/lichess/me/seek")));
</script>

<template>
  <form class="seek" @submit.prevent="find">
    <template v-if="seeking">
      <p class="looking" role="status">
        <ESpinner size="sm" silent />
        <span>Looking for an opponent…</span>
        <span class="meta num">
          {{ clockLabel(seeking.clock_seconds, seeking.increment) }} · {{ seeking.rated ? "rated" : "casual" }}
        </span>
      </p>
      <EButton variant="outline" :loading="busy" @click="cancel">Cancel</EButton>
    </template>
    <template v-else>
      <ESegmented
        v-model="clock"
        label="Clock"
        size="sm"
        :options="clocks"
        :hint="`${speed}, minutes plus seconds a move.`"
      />
      <ESegmented v-model="color" label="You play" size="sm" :options="colours" />
      <ESwitch v-model="rated" label="Rated" />
      <EDisclosure title="Opponent rating range">
        <div class="pair">
          <ENumberField v-model="low" label="From" size="sm" :min="0" :max="4000" :step="50" />
          <ENumberField v-model="high" label="To" size="sm" :min="0" :max="4000" :step="50" />
        </div>
      </EDisclosure>
      <ENotice v-if="rangeProblem" tone="warning" title="Range not used" :role="false">{{ rangeProblem }}</ENotice>
      <EButton type="submit" :loading="busy">Find a game</EButton>
    </template>
    <ENotice v-if="error" tone="danger" title="That did not work" :role="false">{{ error }}</ENotice>
    <p class="why">
      Lichess only allows <Term of="lichessSpeed" inline>rapid and slower</Term> games from apps; challenge a friend for
      blitz.
    </p>
  </form>
</template>

<style scoped>
.seek {
  display: grid;
  justify-items: start;
  gap: var(--e-gap-base);
}

.seek > :deep(.e-field),
.seek > :deep(fieldset),
.seek > :deep(.e-segmented) {
  max-inline-size: 100%;
}

.looking {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--e-gap-snug) var(--e-gap-base);
  margin: 0;
}

.meta,
.why {
  color: var(--e-muted);
  font: var(--e-type-small);
}

.why {
  margin: 0;
}

.pair {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--e-gap-base);
}
</style>
