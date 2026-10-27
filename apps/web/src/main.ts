import { createApp } from "vue";
import { createPinia } from "pinia";
// Components are imported by their own paths, never the barrel, so each part of the app loads only what it uses.
import { createEos } from "@eos-ui/vue/plugin";
import "@eos-ui/vue/icons/builtin";
import "@eos-ui/vue/style.css";
// Self-hosted, so the app the server embeds works offline.
import "@fontsource-variable/fraunces/opsz.css";
import "@fontsource-variable/hanken-grotesk";
import "@fontsource-variable/jetbrains-mono";
import "./base.css";
import { useTransport } from "./api/client";
import { fixtureMode } from "./api/mode";
import App from "./App.vue";
import { prefetchSheets, router } from "./router";
import { startScheme } from "./scheme";
import { startStores } from "./stores/boot";
import { saddleLight, themeCss } from "./theme";

// Fixtures stand in for the server before it exists: the client's transport is swapped, nothing above it knows.
if (fixtureMode()) {
  const { fixtureFetch } = await import("./api/fixtures");
  useTransport(fixtureFetch);
}

// Both schemes ship as CSS, chosen by data-scheme on <html>; index.html sets it before the first paint.
startScheme(themeCss);

const app = createApp(App)
  .use(createEos({ locale: "en-US", theme: saddleLight, applyTheme: false }))
  .use(createPinia())
  .use(router);

void startStores();
await router.isReady();
app.mount("#app");
prefetchSheets();
