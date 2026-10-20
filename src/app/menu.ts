// Native menu bar, rebuilt when the language or toggled state changes.
import { Menu, Submenu, MenuItem, CheckMenuItem, PredefinedMenuItem } from '@tauri-apps/api/menu';
import { t, i18n } from './i18n/index.svelte';
import { app } from './state.svelte';
import { dock } from './dock/layout.svelte';
import * as act from './actions';
import { historyText } from './history';

const isMac = navigator.userAgent.includes('Mac');

async function item(text: string, action: () => unknown, accelerator?: string, enabled = true) {
  return MenuItem.new({ text, action: () => void action(), accelerator, enabled });
}

export async function buildMenu() {
  const hasDoc = app.active != null;
  const h = app.active?.info.history;
  const undoLabel = h && h.current > 0 ? historyText(h.labels[h.current]) : '';
  const redoLabel = h && h.current + 1 < h.labels.length ? historyText(h.labels[h.current + 1]) : '';
  const sep = () => PredefinedMenuItem.new({ item: 'Separator' });

  const appMenu = await Submenu.new({
    text: t('app.name'),
    items: [
      await PredefinedMenuItem.new({ item: { About: { name: t('app.name') } }, text: t('menu.app.about') }),
      await sep(),
      await PredefinedMenuItem.new({ item: 'Hide', text: t('menu.app.hide') }),
      await PredefinedMenuItem.new({ item: 'HideOthers' }),
      await PredefinedMenuItem.new({ item: 'ShowAll' }),
      await sep(),
      await PredefinedMenuItem.new({ item: 'Quit', text: t('menu.app.quit') }),
    ],
  });
  const file = await Submenu.new({
    text: t('menu.file'),
    items: [
      await item(t('file.new'), act.newDocument, 'CmdOrCtrl+N'),
      await item(t('file.open'), act.openFile, 'CmdOrCtrl+O'),
      await sep(),
      await item(t('file.save'), () => act.saveDocument(), 'CmdOrCtrl+S', hasDoc),
      await item(t('file.saveAs'), () => act.saveDocument(app.active, true), 'CmdOrCtrl+Shift+S', hasDoc),
      await item(t('file.exportAs'), () => (app.dialog = 'export'), 'CmdOrCtrl+Alt+Shift+W', hasDoc),
      await sep(),
      await item(t('file.close'), () => act.closeDocument(), 'CmdOrCtrl+W', hasDoc),
    ],
  });
  const edit = await Submenu.new({
    text: t('menu.edit'),
    items: [
      await item(undoLabel ? `${t('edit.undo')}: ${undoLabel}` : t('edit.undo'), act.history.undo, 'CmdOrCtrl+Z', !!undoLabel),
      await item(redoLabel ? `${t('edit.redo')}: ${redoLabel}` : t('edit.redo'), act.history.redo, 'CmdOrCtrl+Shift+Z', !!redoLabel),
      await sep(),
      await PredefinedMenuItem.new({ item: 'Cut', text: t('edit.cut') }),
      await PredefinedMenuItem.new({ item: 'Copy', text: t('edit.copy') }),
      await PredefinedMenuItem.new({ item: 'Paste', text: t('edit.paste') }),
    ],
  });
  const viewMenu = await Submenu.new({
    text: t('menu.view'),
    items: [
      await item(t('view.zoomIn'), act.view.zoomIn, 'CmdOrCtrl+=', hasDoc),
      await item(t('view.zoomOut'), act.view.zoomOut, 'CmdOrCtrl+-', hasDoc),
      await item(t('view.fit'), act.view.fit, 'CmdOrCtrl+0', hasDoc),
      await item(t('view.actual'), act.view.actual, 'CmdOrCtrl+1', hasDoc),
      await item(t('view.resetRotation'), act.view.resetRotation, undefined, hasDoc),
      await sep(),
      await CheckMenuItem.new({ text: t('view.rulers'), checked: app.showRulers, accelerator: 'CmdOrCtrl+R', action: act.view.toggleRulers }),
      await CheckMenuItem.new({ text: t('view.grid'), checked: app.showGrid, accelerator: "CmdOrCtrl+'", action: act.view.toggleGrid }),
      await CheckMenuItem.new({ text: t('view.guides'), checked: app.showGuides, accelerator: 'CmdOrCtrl+;', action: act.view.toggleGuides }),
      await CheckMenuItem.new({ text: t('view.pixelGrid'), checked: app.showPixelGrid, action: act.view.togglePixelGrid }),
      await item(t('view.clearGuides'), act.view.clearGuides, undefined, hasDoc),
    ],
  });
  const windowMenu = await Submenu.new({
    text: t('menu.window'),
    items: [
      ...(await Promise.all((['layers', 'properties', 'adjustments', 'history', 'color', 'brushes', 'navigator'] as const).map((p) => item(t(`panel.${p}`), () => dock.show(p))))),
      await sep(),
      await item(t('window.resetWorkspace'), () => dock.reset()),
      await item(t('window.language'), () => i18n.toggle()),
    ],
  });
  const layerMenu = await Submenu.new({
    text: t('menu.layer'),
    items: [
      await item(t('layer.new'), act.layer.new, 'CmdOrCtrl+Shift+N', hasDoc),
      await item(t('layer.newGroup'), act.layer.newGroup, undefined, hasDoc),
      await item(t('layer.duplicate'), act.layer.duplicate, 'CmdOrCtrl+J', hasDoc),
      await item(t('layer.delete'), act.layer.delete, undefined, hasDoc),
      await sep(),
      await item(t('layer.group'), act.layer.group, 'CmdOrCtrl+G', hasDoc),
      await item(t('layer.ungroup'), act.layer.ungroup, 'CmdOrCtrl+Shift+G', hasDoc),
      await item(t('layer.clip'), act.layer.toggleClip, 'CmdOrCtrl+Alt+G', hasDoc),
      await sep(),
      await item(t('layer.mergeDown'), act.layer.mergeDown, 'CmdOrCtrl+E', hasDoc),
      await item(t('layer.mergeVisible'), act.layer.mergeVisible, 'CmdOrCtrl+Shift+E', hasDoc),
      await item(t('layer.flatten'), act.layer.flatten, undefined, hasDoc),
    ],
  });
  const placeholders = await Promise.all(
    (['menu.image', 'menu.select', 'menu.filter'] as const).map(async (k) =>
      Submenu.new({ text: t(k), items: [await item(t('tool.notYet'), () => {}, undefined, false)] }),
    ),
  );
  const items = [file, edit, placeholders[0], layerMenu, placeholders[1], placeholders[2], viewMenu, windowMenu];
  const menu = await Menu.new({ items: isMac ? [appMenu, ...items] : items });
  await menu.setAsAppMenu();
}
