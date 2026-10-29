<script setup lang="ts">
import {
  formatDate,
  formatDateTime,
  formatDuration,
  formatElo,
  formatEloPm,
  formatNumber,
  formatPercent,
  formatRating,
  formatRelative,
  formatResult,
  formatScore,
} from "../../format";
import UiSection from "../UiSection.vue";
import UiTable from "../UiTable.vue";

/** What every canonical formatter writes for a spread of inputs, so a figure reads alike wherever it appears. */
const now = Date.parse("2026-10-29T12:00:00Z");
const ago = (seconds: number) => now - seconds * 1000;

const rows = [
  { fn: "formatNumber(3708)", out: formatNumber(3708) },
  { fn: "formatNumber(81.34, 1)", out: formatNumber(81.34, 1) },
  { fn: "formatNumber(-12)", out: formatNumber(-12) },
  { fn: "formatNumber(null)", out: formatNumber(null) },
  { fn: "formatElo(12)", out: formatElo(12) },
  { fn: "formatElo(-5)", out: formatElo(-5) },
  { fn: "formatElo(0.04, 1)", out: formatElo(0.04, 1) },
  { fn: "formatEloPm(3.14, 2.04)", out: formatEloPm(3.14, 2.04) },
  { fn: "formatEloPm(-0.7, 5.9)", out: formatEloPm(-0.7, 5.9) },
  { fn: "formatPercent(81.34)", out: formatPercent(81.34) },
  { fn: "formatPercent(null)", out: formatPercent(null) },
  { fn: "formatRating(1840)", out: formatRating(1840) },
  { fn: "formatRating(1345, true)", out: formatRating(1345, true) },
  { fn: "formatDuration(45)", out: formatDuration(45) },
  { fn: "formatDuration(200)", out: formatDuration(200) },
  { fn: "formatDuration(4800)", out: formatDuration(4800) },
  { fn: "formatDuration(190000)", out: formatDuration(190000) },
  { fn: "formatRelative(5 min ago)", out: formatRelative(ago(300), now) },
  { fn: "formatRelative(a day ago)", out: formatRelative(ago(86400), now) },
  { fn: "formatRelative(in 2 h)", out: formatRelative(ago(-7200), now) },
  { fn: "formatDate(12 Oct)", out: formatDate("2026-10-12T09:00:00Z", new Date(now)) },
  { fn: "formatDate(last year)", out: formatDate("2025-03-02T09:00:00Z", new Date(now)) },
  { fn: "formatDateTime(...)", out: formatDateTime("2026-10-12T14:30:00") },
  { fn: "formatScore({ cp: 62 })", out: formatScore({ cp: 62 }) },
  { fn: "formatScore({ cp: -423 })", out: formatScore({ cp: -423 }) },
  { fn: "formatScore({ mate: 3 })", out: formatScore({ mate: 3 }) },
  { fn: "formatScore({ mate: -2 })", out: formatScore({ mate: -2 }) },
  { fn: "formatResult('1/2-1/2')", out: formatResult("1/2-1/2").text },
  { fn: "formatResult('1-0', 'black')", out: JSON.stringify(formatResult("1-0", "black")) },
];
</script>

<template>
  <UiSection id="format" title="format.ts">
    <template #note>One canonical function for each kind of figure. Missing is always an en dash.</template>
    <UiTable
      label="Formatters"
      :columns="[
        { key: 'fn', label: 'Call' },
        { key: 'out', label: 'Writes', fig: true },
      ]"
      :rows="rows"
      row-key="fn"
    />
  </UiSection>
</template>
