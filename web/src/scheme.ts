import { computed, ref, watch } from "vue";
import { stored } from "./stored";
import { saddleDark, saddleLight } from "./theme";

export type Choice = "system" | "light" | "dark";
const CHOICES: Choice[] = ["system", "light", "dark"];

/** The saved choice. Default System. `index.html` reads the same key before first paint. */
export const choice = stored<Choice>("scheme", "system");
const systemDark = ref(false);

const scheme = computed<"light" | "dark">(() =>
  choice.value === "system" ? (systemDark.value ? "dark" : "light") : choice.value,
);

/** The browser chrome takes the header's own colour. */
const chrome = { light: saddleLight.tokens["surface"], dark: saddleDark.tokens["surface"] };

function paint(value: "light" | "dark") {
  const root = document.documentElement;
  root.dataset.scheme = value;
  root.style.colorScheme = value;
  let meta = document.querySelector<HTMLMetaElement>('meta[name="theme-color"]');
  if (!meta) {
    meta = document.createElement("meta");
    meta.name = "theme-color";
    document.head.append(meta);
  }
  meta.content = chrome[value] ?? "";
}

/** Installs the theme CSS and keeps `<html data-scheme>` in step with the choice and the system. Call once, before mount. */
export function startScheme(css: string) {
  const style = document.createElement("style");
  style.dataset.theme = "saddle";
  style.textContent = css;
  document.head.append(style);
  const query = matchMedia("(prefers-color-scheme: dark)");
  systemDark.value = query.matches;
  query.addEventListener("change", (e) => (systemDark.value = e.matches));
  if (!CHOICES.includes(choice.value)) choice.value = "system";
  watch(scheme, paint, { immediate: true, flush: "sync" });
}
