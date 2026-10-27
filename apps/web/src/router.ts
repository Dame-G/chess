import { createRouter, createWebHistory, type RouteRecordRaw } from "vue-router";

/**
 * The board is always there; every other route is a sheet over it (docs/design.md, "The web app"). `/` names what is
 * on the board in its query; the sheets render in the app's sheet layer.
 */
const routes: RouteRecordRaw[] = [
  { path: "/", name: "board", component: { render: () => null }, meta: { title: "" } },
  { path: "/you", name: "you", component: () => import("./sheets/YouSheet.vue"), meta: { title: "You", sheet: true } },
  {
    path: "/dameless",
    name: "dameless",
    component: () => import("./sheets/DamelessSheet.vue"),
    meta: { title: "Dameless", sheet: true },
  },
  {
    path: "/dameless/ideas/:id(\\d+)",
    name: "idea",
    component: () => import("./sheets/IdeaSheet.vue"),
    meta: { title: "Idea", sheet: true },
  },
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
