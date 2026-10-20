import { ar } from './ar';
import { en } from './en';

export type Locale = 'ar' | 'en';
export type Key = keyof typeof ar;

const tables: Record<Locale, Record<Key, string>> = { ar, en };
const STORE = 'mosamem.locale';

function initial(): Locale {
  try {
    const v = localStorage.getItem(STORE);
    if (v === 'ar' || v === 'en') return v;
  } catch {
    /* storage unavailable */
  }
  return 'ar';
}

let current = $state<Locale>(initial());
apply(initial());

function apply(l: Locale) {
  document.documentElement.lang = l;
  document.documentElement.dir = l === 'ar' ? 'rtl' : 'ltr';
}

export const i18n = {
  get locale() {
    return current;
  },
  get dir(): 'rtl' | 'ltr' {
    return current === 'ar' ? 'rtl' : 'ltr';
  },
  set(l: Locale) {
    current = l;
    apply(l);
    try {
      localStorage.setItem(STORE, l);
    } catch {
      /* storage unavailable */
    }
  },
  toggle() {
    this.set(current === 'ar' ? 'en' : 'ar');
  },
};

/**
 * Translates a key; `{name}` placeholders are filled from `vars`. Numbers use Western digits.
 * Text values are wrapped in Unicode isolates (FSI…PDI) so a Latin file name inside an Arabic
 * sentence (or the reverse) can't reorder the punctuation around it.
 */
export function t(key: Key, vars?: Record<string, string | number>): string {
  let s = tables[current][key] ?? key;
  if (vars) for (const [k, v] of Object.entries(vars)) s = s.replaceAll(`{${k}}`, typeof v === 'number' ? fmt(v) : `\u2068${v}\u2069`);
  return s;
}

/** Formats a number for the UI (Western digits in both languages; fields stay typeable). */
export function fmt(n: number, digits = 0): string {
  return n.toLocaleString('en-US', { maximumFractionDigits: digits, useGrouping: false });
}
