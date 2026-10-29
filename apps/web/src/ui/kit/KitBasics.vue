<script setup lang="ts">
import { EButton } from "@eos-ui/vue/components/EButton";
import { PhClockCounterClockwise, PhGameController, PhRobot } from "@phosphor-icons/vue";
import { ref } from "vue";
import type { Score } from "../../api/types";
import type { Result } from "../../format";
import { healthTone, ideaTone, trialTone, workTone, type BadgeTone } from "../badge";
import UiBadge from "../UiBadge.vue";
import UiLive from "../UiLive.vue";
import UiPlayer from "../UiPlayer.vue";
import UiResult from "../UiResult.vue";
import UiScore from "../UiScore.vue";
import UiSection from "../UiSection.vue";
import UiSeg, { type SegOption } from "../UiSeg.vue";
import UiStat from "../UiStat.vue";
import UiStats from "../UiStats.vue";
import { botSeat, damelessSeat, levelSeat, longSeat, masterSeat, nameOnly, youSeat } from "./data";

const tones: BadgeTone[] = ["neutral", "win", "loss", "warn", "info", "act"];

const results: (Result | null)[] = ["1-0", "0-1", "1/2-1/2", "*", null];
const scores: (Score | number | null)[] = [
  { cp: 62 },
  { cp: -423 },
  { cp: 0 },
  { cp: 3 },
  { mate: 3 },
  { mate: -2 },
  { mate: 0 },
  1.4,
  null,
];

const pulse = ref(0);

const time = ref("blitz");
const speeds: SegOption<string>[] = [
  { value: "bullet", label: "Bullet" },
  { value: "blitz", label: "Blitz", count: 412 },
  { value: "rapid", label: "Rapid", count: 38 },
  { value: "classical", label: "Classical", disabled: true },
];
const side = ref("white");
const filter = ref("all");
const many = ref("c");
const marks = ref(["blitz"]);
const icons = ref("play");
const manyOptions = ["a", "b", "c", "d", "e", "f", "g", "h"].map((v) => ({
  value: v,
  label: `Option ${v.toUpperCase()}`,
}));
</script>

<template>
  <UiSection id="stat" title="UiStat and UiStats">
    <template #note>A label, the figure, a unit, a delta, a line under. A missing value is an en dash.</template>
    <UiStats>
      <UiStat label="Rating" :value="1840" delta="+12" />
      <UiStat label="Games" :value="3708" />
      <UiStat label="Score" :value="62.4" unit="%" />
      <UiStat label="Accuracy" :value="null" hint="accuracy" />
      <UiStat label="Elo" :value="-3.4" hint="elo" tone="loss" sub="± 5.9, 412 pairs" />
      <UiStat label="Won" :value="214" tone="win" :delta="-3" />
    </UiStats>
    <UiStats :columns="3" dense>
      <UiStat label="Dense" :value="88" unit="min" />
      <UiStat label="Plain tooltip" value="v14" hint="The version Dameless plays now." />
      <UiStat label="Custom value"><UiScore :score="{ cp: 62 }" /></UiStat>
    </UiStats>
    <UiStats :columns="3">
      <UiStat label="Medium" :value="1840" />
      <UiStat label="Large" :value="1840" size="lg" />
      <UiStat label="The hero figure" :value="1840" size="xl" tone="info" />
    </UiStats>
  </UiSection>

  <UiSection id="badge" title="UiBadge">
    <template #note>Status as a word, tinted, sentence case. Solid is for LIVE and nothing else.</template>
    <div class="row">
      <UiBadge v-for="t in tones" :key="t" :tone="t">{{ t }}</UiBadge>
    </div>
    <div class="row">
      <UiBadge v-for="t in tones" :key="t" :tone="t" dot>{{ t }}</UiBadge>
    </div>
    <div class="row">
      <UiBadge v-for="t in tones" :key="t" :tone="t" solid>{{ t }}</UiBadge>
    </div>
    <div class="row">
      <UiBadge v-for="(tone, s) in ideaTone" :key="s" :tone="tone">{{ s }}</UiBadge>
      <UiBadge v-for="(tone, s) in trialTone" :key="s" :tone="tone">{{ s }}</UiBadge>
      <UiBadge v-for="(tone, s) in workTone" :key="s" :tone="tone">{{ s }}</UiBadge>
      <UiBadge v-for="(h, s) in healthTone" :key="s" :tone="h.tone">{{ h.word }}</UiBadge>
    </div>
  </UiSection>

  <UiSection id="result" title="UiResult">
    <template #note>From your side when you played: green won, red lost. A stranger's game stays ink.</template>
    <div class="kit-table">
      <span class="muted">Result</span>
      <span class="muted">No side</span>
      <span class="muted">You: white</span>
      <span class="muted">You: black</span>
      <span class="muted">Letters (white)</span>
      <span class="muted">Dots (white)</span>
      <template v-for="r in results" :key="String(r)">
        <code class="mono">{{ r ?? "null" }}</code>
        <UiResult :result="r" />
        <UiResult :result="r" you="white" />
        <UiResult :result="r" you="black" />
        <UiResult :result="r" you="white" format="letter" />
        <UiResult :result="r" you="white" format="dot" />
      </template>
    </div>
  </UiSection>

  <UiSection id="score" title="UiScore">
    <template #note>
      The figure has no colour: White being ahead is not "good". A pip names the side. From the mover's side it is an
      outcome and takes the colours.
    </template>
    <div class="kit-table kit-table--4">
      <span class="muted">Score</span>
      <span class="muted">White's point of view</span>
      <span class="muted">Small</span>
      <span class="muted">Mover's point of view</span>
      <template v-for="(s, i) in scores" :key="i">
        <code class="mono">{{ JSON.stringify(s) }}</code>
        <UiScore :score="s" />
        <UiScore :score="s" size="sm" />
        <UiScore :score="s" perspective="mover" />
      </template>
    </div>
  </UiSection>

  <UiSection id="player" title="UiPlayer">
    <template #note>One way to name a player: side, name, title, rating (a "?" only while provisional).</template>
    <div class="kit-list">
      <UiPlayer :seat="youSeat" colour="white" />
      <UiPlayer :seat="damelessSeat" colour="black" />
      <UiPlayer :seat="levelSeat" />
      <UiPlayer :seat="botSeat" colour="white" />
      <UiPlayer :seat="masterSeat" colour="black" :rating="2105" provisional />
      <UiPlayer :seat="nameOnly" :rating="null" />
      <UiPlayer :seat="botSeat" link />
      <UiPlayer :seat="damelessSeat" live colour="white" />
      <div class="kit-narrow"><UiPlayer :seat="longSeat" colour="black" /></div>
    </div>
    <div class="row">
      <UiPlayer :seat="{ name: 'You', key: 'you', rating: 1840 }" size="md" link current />
      <UiPlayer :seat="{ name: 'Dameless v14', key: 'dameless', rating: 3290 }" size="md" link />
    </div>
  </UiSection>

  <UiSection id="live" title="UiLive">
    <template #note>A dot. Pulses once when the value of `pulse` changes, not for ever.</template>
    <div class="row">
      <UiLive tone="act" :pulse="pulse" label="Live" /> <span class="muted">act</span> <UiLive tone="win" />
      <span class="muted">connected</span> <UiLive tone="loss" /> <span class="muted">error</span>
      <EButton variant="outline" size="sm" @click="pulse++">Pulse (a move was played)</EButton>
    </div>
  </UiSection>

  <UiSection id="seg" title="UiSeg">
    <template #note>The one segmented control. Arrow keys move and choose; tab goes in and out.</template>
    <div class="stack">
      <div class="row">
        <UiSeg v-model="time" label="Speed" :options="speeds" />
        <UiSeg
          v-model="side"
          label="Colour"
          size="sm"
          :options="[
            { value: 'white', label: 'As White' },
            { value: 'black', label: 'As Black' },
          ]"
        />
        <UiSeg
          v-model="filter"
          label="Show"
          size="sm"
          :options="[
            { value: 'all', label: 'All' },
            { value: 'you', label: 'You', count: 12 },
            { value: 'trials', label: 'Trials', count: 308 },
          ]"
        />
      </div>
      <UiSeg
        v-model="filter"
        label="Show (full width)"
        full
        :options="[
          { value: 'all', label: 'All' },
          { value: 'you', label: 'You' },
          { value: 'dameless', label: 'Dameless' },
          { value: 'lichess', label: 'Lichess' },
          { value: 'trials', label: 'Trials' },
        ]"
      />
      <UiSeg
        v-model="icons"
        label="With icons"
        :options="[
          { value: 'play', label: 'Play', icon: PhGameController },
          { value: 'bot', label: 'Bot', icon: PhRobot },
          { value: 'history', label: 'History', icon: PhClockCounterClockwise },
        ]"
      />
      <UiSeg v-model:values="marks" label="Speeds" multiple :options="speeds" />
      <div class="kit-narrow">
        <UiSeg v-model="many" label="Many options scroll inside their track" :options="manyOptions" />
      </div>
    </div>
  </UiSection>
</template>

<style scoped>
.kit-table {
  display: grid;
  grid-template-columns: repeat(6, minmax(0, auto));
  gap: var(--sp-2) var(--sp-4);
  align-items: center;
  justify-content: start;
}

.kit-table--4 {
  grid-template-columns: repeat(4, minmax(0, auto));
}

.kit-table .muted {
  font: var(--t-meta);
}

.kit-list {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(14rem, 1fr));
  gap: var(--sp-3);
}

.kit-narrow {
  max-width: 14rem;
}
</style>
