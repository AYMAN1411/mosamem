<script lang="ts">
  // Help › Keyboard Shortcuts: the tool letters, the single keys, and the menu shortcuts.
  import Modal from './Modal.svelte';
  import { app } from '../state.svelte';
  import { t } from '../i18n/index.svelte';
  import { KEY_SHORTCUTS, MENU_SHORTCUTS, showKeys, toolShortcuts } from '../shortcutSheet';

  const mac = navigator.userAgent.includes('Mac');
  const tools = toolShortcuts();
</script>

<Modal title={t('shortcuts.title')} onclose={() => (app.dialog = null)} width={760}>
  <div class="sheet">
    <section>
      <h3>{t('shortcuts.tools')}</h3>
      <dl>
        {#each tools as s (s.label)}<dt>{t(s.label)}</dt><dd><kbd>{s.keys}</kbd></dd>{/each}
      </dl>
      <h3>{t('shortcuts.keys')}</h3>
      <dl>
        {#each KEY_SHORTCUTS as s (s.label)}<dt>{t(s.label)}</dt><dd><kbd>{showKeys(s.keys, mac)}</kbd></dd>{/each}
      </dl>
    </section>
    {#each MENU_SHORTCUTS as sec (sec.title)}
      <section>
        <h3>{t(sec.title)}</h3>
        <dl>
          {#each sec.items as s (s.keys)}<dt>{t(s.label).replace('…', '')}</dt><dd><kbd>{showKeys(s.keys, mac)}</kbd></dd>{/each}
        </dl>
      </section>
    {/each}
  </div>
  <p class="note">{t('shortcuts.note')}</p>
</Modal>

<style>
  .sheet { columns: 3 220px; column-gap: 24px; max-height: 64vh; overflow: auto; }
  section { break-inside: avoid; margin-bottom: 14px; }
  h3 { margin: 0 0 6px; font-size: var(--fs-1); font-weight: 600; color: var(--fg-0); }
  dl { display: grid; grid-template-columns: 1fr auto; gap: 3px 10px; margin: 0 0 10px; font-size: var(--fs-0); }
  dt { color: var(--fg-1); }
  dd { margin: 0; text-align: end; }
  kbd { direction: ltr; display: inline-block; font-family: inherit; padding: 1px 6px; border-radius: 4px; background: var(--bg-4); border: 1px solid var(--line-strong); color: var(--fg-0); white-space: nowrap; }
  .note { margin: 8px 0 0; color: var(--fg-3); font-size: var(--fs-0); }
</style>
