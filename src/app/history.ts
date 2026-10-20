// Text for history steps: the label key translated, in the current UI language.
import type { HistoryLabel } from '../engine/bridge';
import { t, type Key } from './i18n/index.svelte';

export function historyText(l: HistoryLabel | undefined): string {
  if (!l) return '';
  return t(l.key as Key);
}
