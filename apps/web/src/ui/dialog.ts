/** Things that keep the Enter key for themselves: a button presses, a link follows, a combobox picks. */
const KEEPS_ENTER =
  'button, a, summary, [role="combobox"], [role="listbox"], [role="option"], [contenteditable="true"]';

/**
 * Whether this key press should confirm a dialog: plain Enter from a field, or Ctrl/Cmd+Enter from a textarea (where
 * plain Enter is a new line). Composing text (an IME) and a press something else already handled never confirm.
 */
export function confirmsOnEnter(e: KeyboardEvent): boolean {
  if (e.key !== "Enter" || e.defaultPrevented || e.isComposing) return false;
  const target = e.target;
  if (!(target instanceof Element)) return false;
  const mod = e.ctrlKey || e.metaKey;
  if (target.closest("textarea") ? !mod : mod || e.shiftKey || e.altKey) return false;
  return !target.closest(KEEPS_ENTER);
}
