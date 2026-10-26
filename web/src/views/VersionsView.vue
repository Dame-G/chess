<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useQueryCache } from "@pinia/colada";
import { PhDotsThree, PhList, PhMagnifyingGlass, PhTrash } from "@phosphor-icons/vue";
import { useShortcuts } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ECheckbox } from "@eos-ui/vue/components/ECheckbox";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";
import { EMenu, type EMenuItem } from "@eos-ui/vue/components/EMenu";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ESearchBar } from "@eos-ui/vue/components/ESearchBar";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { api } from "../api/runner";
import { unwrap, useBuilds, useVersions, versionPath } from "../api/queries";
import { speed } from "../format";
import { stored } from "../stored";
import Workspace from "../components/Workspace.vue";
import Term from "../components/Term.vue";
import DeleteVersion from "../components/versions/DeleteVersion.vue";
import FreeSpace from "../components/versions/FreeSpace.vue";
import SpeedBars from "../components/versions/SpeedBars.vue";
import LibraryHead from "../components/improve/LibraryHead.vue";
import { stepTarget, toLibrary } from "../components/improve/libraryNav";
import VersionRows from "../components/versions/VersionRows.vue";
import VersionStage from "../components/versions/VersionStage.vue";
import { deletable, libraryOf, liveBuilds, resolveVersion, sortVersions } from "../components/versions/versions";

const route = useRoute();
const router = useRouter();
const cache = useQueryCache();
const { state: versions } = useVersions();
const { state: builds } = useBuilds();

const param = computed(() => String(route.params.name ?? ""));
const all = computed(() => versions.value.data ?? []);
const current = computed(() => (param.value ? resolveVersion(all.value, param.value) : undefined));

// ---- Library
const search = ref("");
const showHidden = stored("versions.hidden", false);
const hiddenCount = computed(() => all.value.filter((v) => v.hidden || v.upstream).length);
const shown = computed(() => libraryOf(all.value, search.value, showHidden.value, current.value?.name));

// Selection lives in the URL. With none, the newest version of the fork that is not main opens. An old address that
// named a build number moves to the branch that owns it.
watch(
  [all, param],
  () => {
    if (!all.value.length) return;
    if (!param.value) {
      const list = sortVersions(all.value).filter((v) => !v.hidden);
      const pick = list.find((v) => !v.is_main && !v.upstream) ?? list[0];
      if (pick) void router.replace({ path: versionPath(pick.name), query: route.query });
    } else if (current.value && current.value.name !== param.value) {
      void router.replace({ path: versionPath(current.value.name), query: route.query });
    }
  },
  { immediate: true },
);

const listEl = ref<HTMLElement>();
watch(
  () => current.value?.name,
  async () => {
    await nextTick();
    // Stacked, the list is below the stage and scrolling to it would move the page.
    if (!matchMedia("(width >= 75rem)").matches) return;
    listEl.value?.querySelector("[aria-current]")?.scrollIntoView({ block: "nearest" });
  },
);

function step(delta: number, event: KeyboardEvent) {
  const at = shown.value.findIndex((v) => v.name === current.value?.name);
  const next = stepTarget(shown.value, at, delta, event);
  if (next) void router.replace({ path: versionPath(next.name), query: route.query });
}
const freeing = ref(false);
const deleting = ref(false);
const error = ref("");
const dialogs = computed(() => deleting.value || freeing.value);
useShortcuts(
  { arrowdown: (e) => step(1, e), arrowup: (e) => step(-1, e) },
  { enabled: () => !dialogs.value, preventDefault: false },
);

// ---- Tidying

async function setHidden(hidden: boolean) {
  const v = current.value;
  if (!v) return;
  error.value = "";
  try {
    await unwrap(api.PUT("/api/versions/{name}/hidden", { params: { path: { name: v.name } }, body: { hidden } }));
    void cache.invalidateQueries({ key: ["versions"] });
    void cache.invalidateQueries({ key: ["branches"] });
  } catch (e) {
    error.value = (e as Error).message;
  }
}

const stageMenu = computed<EMenuItem[]>(() => {
  const v = current.value;
  if (!v) return [];
  const items: EMenuItem[] = [];
  if (v.latest_build_id != null)
    items.push({
      label: "Analyse with it",
      icon: PhMagnifyingGlass,
      onClick: () => void router.push({ path: "/board", query: { build: String(v.latest_build_id) } }),
    });
  if (!v.is_main)
    items.push({ label: v.hidden ? "Show in the list" : "Hide from the list", onClick: () => setHidden(!v.hidden) });
  if (deletable(v))
    items.push(
      { divider: true },
      { label: "Delete branch…", icon: PhTrash, danger: true, onClick: () => (deleting.value = true) },
    );
  return items;
});
const libraryMenu = computed<EMenuItem[]>(() => [{ label: "Free disk space…", onClick: () => (freeing.value = true) }]);

// ---- Inspector: speed of each version's newest build, and what the fingerprint is
const bars = computed(() =>
  sortVersions(all.value)
    .filter((v) => (!v.hidden && !v.upstream) || v.name === current.value?.name)
    .flatMap((v) => {
      const b = liveBuilds(v)[0];
      return b ? [{ name: v.name, nps: b.nps }] : [];
    }),
);
const pace = computed(() => {
  const mine = bars.value.find((b) => b.name === current.value?.name);
  if (!mine || bars.value.length < 2) return "";
  const gap = 1 - mine.nps / Math.max(...bars.value.map((b) => b.nps));
  return gap < 0.01 ? "The fastest version." : `${Math.round(gap * 100)}% slower than the fastest.`;
});
const latest = computed(() => {
  const id = current.value?.latest_build_id;
  return id == null ? undefined : builds.value.data?.find((b) => b.id === id);
});
const fingerprint = computed(() => {
  const b = current.value?.builds.find((x) => x.id === current.value?.latest_build_id);
  if (!b) return "";
  if (b.bench_ok == null) return "Its commit has no Bench line, so there is nothing to check the build against.";
  if (b.bench_ok) return "The build counts the nodes its commit says, so it plays as intended.";
  return `The build counted ${b.bench.toLocaleString("en")} nodes where its commit says otherwise. Either the message is out of date or the engine does not search the same way twice. Check before testing with it.`;
});
</script>

<template>
  <Workspace library-label="Versions" inspector-label="Speed">
    <template #library-head>
      <LibraryHead title="Versions">
        <template #action>
          <EMenu label="Library options" :items="libraryMenu" align="end">
            <template #trigger>
              <EIconButton :icon="PhDotsThree" label="Library options" size="sm" />
            </template>
          </EMenu>
        </template>
        <ESearchBar v-model="search" label="Search versions" placeholder="Search" size="sm" hide-action />
        <ECheckbox v-if="hiddenCount" v-model="showHidden" :label="`Show hidden and upstream (${hiddenCount})`" />
      </LibraryHead>
    </template>

    <template #library>
      <ESkeleton v-if="versions.status === 'pending'" :lines="5" label="Loading versions" />
      <ENotice
        v-else-if="versions.status === 'error'"
        class="pad"
        tone="danger"
        title="Could not load versions"
        :role="false"
      >
        {{ versions.error?.message }}
      </ENotice>
      <EEmptyState v-else-if="!shown.length" bare size="sm" title="No versions here">
        {{ all.length ? "Try another word, or show the hidden ones." : "The engine's checkout has no branches yet." }}
      </EEmptyState>
      <div v-else ref="listEl">
        <VersionRows :versions="shown" :current="current?.name ?? ''" :query="search" />
      </div>
    </template>

    <template v-if="current" #stage-head>
      <h2 class="title">{{ current.name }}</h2>
      <span class="grow"></span>
      <EButton class="to-library" variant="quiet" size="sm" :icon="PhList" @click="toLibrary('Versions')"
        >Library</EButton
      >
      <EMenu v-if="stageMenu.length" label="More about this version" :items="stageMenu" align="end">
        <template #trigger>
          <EIconButton :icon="PhDotsThree" label="More about this version" size="sm" />
        </template>
      </EMenu>
    </template>

    <ENotice
      v-if="error"
      class="pad"
      tone="danger"
      title="That did not work"
      closable
      :role="false"
      @close="error = ''"
    >
      {{ error }}
    </ENotice>
    <VersionStage v-if="current" :version="current" />
    <ESkeleton v-else-if="versions.status === 'pending' || all.length" class="pad" :lines="6" label="Loading" />
    <div v-else class="pad">
      <EEmptyState title="No versions" icon="info">
        A version is a branch of the engine. <Term of="branch" inline>What is a branch?</Term>
      </EEmptyState>
    </div>

    <template v-if="current" #inspector-head>
      <h2 class="ins-title">Speed</h2>
    </template>
    <template v-if="current" #inspector>
      <div class="ins">
        <section class="part" aria-labelledby="ins-speed">
          <h3 id="ins-speed" class="part-title">Newest build of each version</h3>
          <p v-if="bars.length" class="note">
            <b v-if="pace">{{ pace }}</b> Measured on this machine; runs of the same commit differ by a few percent.
          </p>
          <SpeedBars v-if="bars.length" :bars="bars" :selected="current.name" />
          <p v-else class="note">Nothing is built yet.</p>
        </section>
        <section v-if="fingerprint" class="part" aria-labelledby="ins-print">
          <h3 id="ins-print" class="part-title">Fingerprint</h3>
          <p class="note">
            {{ fingerprint }}
            <Term of="bench" inline>What is a fingerprint?</Term>
          </p>
          <p v-if="latest" class="note num">
            {{ latest.network ?? "No network embedded" }} ·
            {{ latest.features.includes("no-syzygy") ? "no tablebases" : "tablebases" }} ·
            {{ speed(latest.nps) }}
          </p>
        </section>
      </div>
    </template>
  </Workspace>

  <FreeSpace v-model:open="freeing" />
  <DeleteVersion v-if="current && deletable(current)" v-model:open="deleting" :version="current" />
</template>

<style scoped>
.title,
.ins-title {
  margin: 0;
  font: var(--e-type-h5);
}

.title {
  flex: 0 1 auto;
  min-width: 0;
  overflow: hidden;
  font: var(--e-type-h4);
  font-family: var(--e-font-mono);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.grow {
  flex: 1;
}

.to-library {
  display: none;
}

@media (width < 75rem) {
  .to-library {
    display: inline-flex;
  }
}

.pad {
  margin: var(--e-gap-loose) var(--ws-pad);
  align-self: start;
}

.ins {
  display: grid;
  gap: var(--e-gap-section);
  container-type: inline-size;
  padding: var(--ws-pad, var(--e-pad-control));
}

.part {
  display: grid;
  gap: var(--e-gap-base);
  min-inline-size: 0;
}

.part-title {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.note {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
