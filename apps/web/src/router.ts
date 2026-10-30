import { createRouter, createWebHistory, type RouteRecordRaw } from "vue-router";

/**
 * The board is always there; every other route is a sheet over it (docs/design.md, "The web app"). `/` names what is
 * on the board in its query; the sheets render in the app's sheet layer.
 */
const routes: RouteRecordRaw[] = [
  { path: "/", name: "board", component: { render: () => null }, meta: { title: "" } },
  { path: "/you", name: "you", component: () => import("./sheets/YouSheet.vue"), meta: { title: "You", sheet: true } },
  {
    path: "/players/:key",
    name: "player",
    component: () => import("./sheets/PlayerSheet.vue"),
    meta: { title: "Player", sheet: true },
  },
  {
    path: "/compare",
    name: "compare",
    component: () => import("./sheets/CompareSheet.vue"),
    meta: { title: "Compare", sheet: true },
  },
  {
    path: "/settings",
    name: "settings",
    component: () => import("./sheets/SettingsSheet.vue"),
    meta: { title: "Settings", sheet: true },
  },
  { path: "/:rest(.*)*", redirect: "/" },
];

export const router = createRouter({ history: createWebHistory(), routes });

/**
 * Once the board has painted and the browser is idle, fetch every sheet's chunk, so opening a sheet never waits for
 * the network and the board does not show through an empty sheet layer meanwhile.
 */
export function prefetchSheets() {
  const load = () => {
    for (const route of routes) {
      const component = route.component;
      if (route.meta?.["sheet"] && typeof component === "function") void (component as () => Promise<unknown>)();
    }
  };
  if ("requestIdleCallback" in window) window.requestIdleCallback(load, { timeout: 4000 });
  else setTimeout(load, 1500);
}
