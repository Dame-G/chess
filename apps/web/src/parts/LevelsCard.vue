<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import { ETable } from "@eos-ui/vue/components/ETable";
import { calibrationCost, isCalibrated, levelName, levelRatingText } from "../sheets/levels";
import { useDameless } from "../stores/dameless";
import { usePolicy } from "../stores/policy";
import { useWork } from "../stores/work";
import Term from "./Term.vue";

/**
 * Levels: the ladder Dameless can play at, each with the rating the app has found for it by playing (or the guess it
 * started from, marked as an estimate), how many games went into it, and Calibrate levels, which queues those games
 * in the background. Your own place on the ladder comes from your games against it.
 */
const dameless = useDameless();
const policy = usePolicy();
const work = useWork();
const toast = useToast();

const failed = ref<string | null>(null);
onMounted(async () => {
  try {
    await Promise.all([dameless.loadLevels(), dameless.loadPlan()]);
  } catch (e) {
    failed.value = e instanceof Error ? e.message : String(e);
  }
});

const overview = computed(() => dameless.levels);
const levels = computed(() => overview.value?.levels ?? []);
/** Calibration work in the queue or running: one at a time is enough. */
const calibrating = computed(() => [...work.running, ...work.queued].find((w) => w.kind === "calibrate"));
/** Games calibrate in pairs on two cores each. */
const slots = computed(() => Math.max(1, Math.floor((policy.status?.cores ?? 8) / 4)));
const cost = computed(() => calibrationCost(dameless.plan, levels.value, slots.value));
const any = computed(() => levels.value.some(isCalibrated));

const busy = ref(false);
async function calibrate() {
  busy.value = true;
  try {
    const w = await dameless.calibrate();
    toast.show({ title: "Calibration queued", message: w.title, tone: "success" });
  } catch (e) {
    toast.show({
      title: "Calibration was not queued",
      message: e instanceof Error ? e.message : String(e),
      tone: "warning",
    });
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <section class="sec" aria-labelledby="d-levels">
    <div class="sec-head">
      <h2 id="d-levels" class="part-title">Levels</h2>
      <p class="sec-note">
        Dameless at a strength you choose. Each rating is found by <Term k="calibration" text="calibration" />.
      </p>
    </div>
    <ENotice v-if="failed" tone="danger" title="Levels did not load">{{ failed }}</ENotice>
    <div v-else-if="!overview" aria-busy="true"><ESkeleton :lines="4" label="Loading the levels" /></div>
    <template v-else>
      <ETable caption="The levels of Dameless" hide-caption dense>
        <thead>
          <tr>
            <th scope="col">Level</th>
            <th scope="col" class="e-num">Rating</th>
            <th scope="col" class="e-num">Games</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="l in levels" :key="l.level.id">
            <th scope="row">
              {{ levelName(l.level) }}
              <EStamp
                v-if="l.level.id === overview.suggested"
                size="sm"
                tone="info"
                label="Near your rating"
                class="near"
              />
            </th>
            <td class="e-num">
              <span class="mono">{{ levelRatingText(l) }}</span>
              <span v-if="!isCalibrated(l)" class="est">estimate</span>
            </td>
            <td class="e-num mono">{{ l.games }}</td>
          </tr>
          <tr v-if="overview.you" class="you">
            <th scope="row">You</th>
            <td class="e-num mono">
              {{ Math.round(overview.you.estimate.elo) }} ±
              {{ Math.round((overview.you.estimate.high - overview.you.estimate.low) / 2) }}
            </td>
            <td class="e-num mono">{{ overview.you.games }}</td>
          </tr>
        </tbody>
      </ETable>
      <div class="sheet-actions">
        <EButton
          variant="outline"
          size="sm"
          :loading="busy"
          :disabled="!!calibrating || !dameless.plan.length"
          @click="calibrate"
        >
          Calibrate levels
        </EButton>
        <span v-if="calibrating" class="sec-note">{{ calibrating.title }}. The status line shows how it goes.</span>
        <span v-else-if="cost" class="sec-note">{{ cost }}</span>
      </div>
      <p v-if="!any && !calibrating" class="sec-note">
        No level has played yet, so these are the ratings each was designed for. Play a few games against a level, or
        calibrate.
      </p>
    </template>
  </section>
</template>

<style scoped>
.near {
  margin-inline-start: var(--e-gap-snug);
}

.est {
  margin-inline-start: var(--e-gap-snug);
  color: var(--e-muted);
  font: var(--e-type-small);
}

.you th,
.you td {
  border-block-start: var(--line);
  font-weight: 600;
}
</style>
