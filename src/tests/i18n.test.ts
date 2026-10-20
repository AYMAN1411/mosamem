import { describe, expect, it } from 'vitest';
import { ar } from '../app/i18n/ar';
import { en } from '../app/i18n/en';
import { i18n, t } from '../app/i18n/index.svelte';

describe('translations', () => {
  it('Arabic and English have exactly the same keys, none empty', () => {
    expect(Object.keys(en).sort()).toEqual(Object.keys(ar).sort());
    for (const [k, v] of [...Object.entries(ar), ...Object.entries(en)]) expect(v.trim(), k).not.toBe('');
  });

  it('placeholders match between languages', () => {
    const ph = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();
    for (const k of Object.keys(ar) as (keyof typeof ar)[]) expect(ph(en[k]), k).toEqual(ph(ar[k]));
  });

  it('inserted text is bidi-isolated, numbers are not', () => {
    i18n.set('ar');
    expect(t('save.unsavedOne', { name: 'photo.png' })).toBe('هل تريد حفظ التغييرات في «\u2068photo.png\u2069»؟');
  });

  it('switching language flips the document direction', () => {
    i18n.set('ar');
    expect(document.documentElement.dir).toBe('rtl');
    expect(t('status.size', { w: 1080, h: 1350 })).toBe('1080 × 1350 بكسل');
    i18n.set('en');
    expect(document.documentElement.dir).toBe('ltr');
    expect(t('status.size', { w: 1080, h: 1350 })).toBe('1080 × 1350 px');
  });
});
