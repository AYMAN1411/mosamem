<script lang="ts">
  import Logo from './icons/Logo.svelte';
  import Icon from './icons/Icon.svelte';
  import { t } from './i18n/index.svelte';
  import { newDocument, openFile } from './actions';
</script>

<div class="welcome">
  <Logo size={88} />
  <h1>{t('welcome.title')}</h1>
  <p>{t('welcome.subtitle')}</p>
  <div class="actions">
    <button class="btn primary" onclick={newDocument}><Icon name="plus" size={16} />{t('welcome.new')}</button>
    <button class="btn" onclick={openFile}><Icon name="folder" size={16} />{t('welcome.open')}</button>
  </div>
  <p class="hint">{t('welcome.drop')}</p>
</div>

<style>
  .welcome { position: absolute; inset: 0; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; background: var(--bg-0); text-align: center; z-index: 5; }
  h1 { margin: 12px 0 0; font-size: 26px; font-weight: 600; }
  p { margin: 0; color: var(--fg-2); font-size: var(--fs-3); }
  .actions { display: flex; gap: 10px; margin-top: 18px; }
  .actions .btn { height: 36px; padding: 0 18px; font-size: var(--fs-2); }
  .hint { margin-top: 6px; color: var(--fg-3); font-size: var(--fs-1); }
</style>
