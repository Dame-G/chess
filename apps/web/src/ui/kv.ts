import type { InjectionKey } from "vue";

/** Whether the `UiKeyValue` around a `UiKv` is the inline variant (values only, joined by middle dots). */
export const kvInline: InjectionKey<boolean> = Symbol("ui-kv-inline");
