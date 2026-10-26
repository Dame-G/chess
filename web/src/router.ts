import { createRouter, createWebHistory, type RouteLocation } from "vue-router";

/** A redirect from an old address: the new path, with the query kept. */
const moved = (path: (params: Record<string, string>) => string) => (to: RouteLocation) => ({
  path: path(to.params as Record<string, string>),
  query: to.query,
});

const board = () => import("./views/BoardView.vue");
const improve = () => import("./views/ImproveView.vue");
const versions = () => import("./views/VersionsView.vue");

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: "/", name: "home", component: () => import("./views/HomeView.vue"), meta: { workspace: true } },

    // The board: free analysis, a played game with its test's games as the playlist, or a match between two builds.
    { path: "/board", name: "board", component: board, meta: { title: "Board", workspace: true } },
    { path: "/games/:test/:index", name: "game", component: board, meta: { title: "Game", workspace: true } },
    { path: "/matches/:id", name: "match", component: board, meta: { title: "Match", workspace: true } },

    // Improve: tests and tunes in one library, the chosen one on the stage.
    { path: "/improve", name: "improve", component: improve, meta: { title: "Improve", workspace: true } },
    {
      path: "/improve/experiments/:id",
      name: "improve-test",
      component: improve,
      meta: { title: "Improve", workspace: true },
    },
    {
      path: "/improve/tunes/:id",
      name: "improve-tune",
      component: improve,
      meta: { title: "Improve", workspace: true },
    },

    // Versions: the engine's branches, with their builds. A name with a slash is one encoded segment.
    { path: "/versions", name: "versions", component: versions, meta: { title: "Versions", workspace: true } },
    { path: "/versions/:name", name: "version", component: versions, meta: { title: "Versions", workspace: true } },

    // The Lichess bot: its games, rules and challenges.
    {
      path: "/lichess",
      name: "lichess",
      component: () => import("./views/LichessView.vue"),
      meta: { title: "Lichess", workspace: true },
    },

    // Players side by side over their reviewed games. The address keeps who and which games (?players=me,name).
    {
      path: "/lichess/compare",
      name: "compare",
      component: () => import("./views/CompareView.vue"),
      meta: { title: "Compare", workspace: true },
    },

    // Old addresses keep their query at the new path.
    { path: "/analysis", redirect: moved(() => "/board") },
    { path: "/lab", redirect: moved(() => "/improve") },
    { path: "/lab/:id", redirect: moved((p) => `/improve/experiments/${p.id}`) },
    { path: "/experiments", redirect: moved(() => "/improve") },
    { path: "/experiments/:id", redirect: moved((p) => `/improve/experiments/${p.id}`) },
    // A new tune was ?new=1 on the old tune page: it is ?new=tune here.
    {
      path: "/tune",
      redirect: (to) => ({ path: "/improve", query: to.query.new ? { ...to.query, new: "tune" } : to.query }),
    },
    { path: "/tune/:id", redirect: moved((p) => `/improve/tunes/${p.id}`) },
    { path: "/engines", redirect: moved(() => "/versions") },
    // An old engine was a build number; the Versions view finds its branch.
    { path: "/engines/:id", redirect: moved((p) => `/versions/${p.id}`) },
    { path: "/tests/:id/games/:index", redirect: moved((p) => `/games/${p.id}/${p.index}`) },
    { path: "/runner", redirect: "/" },
    { path: "/:rest(.*)*", redirect: "/" },
  ],
});

router.afterEach((to) => {
  document.title = to.meta.title ? `${to.meta.title} · Dameless` : "Dameless";
});

declare module "vue-router" {
  interface RouteMeta {
    title?: string;
    /** Fills the window with panes that scroll on their own, instead of scrolling as a page. */
    workspace?: boolean;
  }
}
