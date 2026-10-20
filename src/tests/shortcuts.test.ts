import { describe, expect, it } from 'vitest';
import menu from '../app/menu.ts?raw';
import { MENU_SHORTCUTS, showKeys } from '../app/shortcutSheet';
import { en } from '../app/i18n/en';

describe('keyboard reference', () => {
  it('lists only shortcuts the menu really has, with the same action', () => {
    for (const s of MENU_SHORTCUTS) {
      for (const { keys, label } of s.items) {
        const line = menu.split('\n').find((l: string) => l.includes(`'${keys}'`) || l.includes(`"${keys}"`));
        expect(line, `${keys} is in menu.ts`).toBeTruthy();
        expect(line, `${keys} runs ${label}`).toContain(`'${label}'`);
      }
    }
  });
  it('has every label in English', () => {
    for (const s of MENU_SHORTCUTS) for (const { label } of s.items) expect((en as Record<string, string>)[label], label).toBeTruthy();
  });
  it('writes keys the platform way', () => {
    expect(showKeys('CmdOrCtrl+Shift+S', true)).toBe('⌘⇧S');
    expect(showKeys('CmdOrCtrl+Shift+S', false)).toBe('Ctrl+Shift+S');
    expect(showKeys('Shift+Backspace', true)).toBe('⇧⌫');
  });
});
