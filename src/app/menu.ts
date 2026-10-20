// Native menu bar, rebuilt when the language or toggled state changes.
import { Menu, Submenu, MenuItem, CheckMenuItem, PredefinedMenuItem } from '@tauri-apps/api/menu';
import { t, i18n, type Key } from './i18n/index.svelte';
import { ADJUSTMENT_KINDS } from '../engine/adjust';
import { FILTER_MENU } from '../engine/filter';
import { app } from './state.svelte';
import { dock } from './dock/layout.svelte';
import * as act from './actions';
import { historyText } from './history';
import { findLayer } from '../engine/bridge';

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
      await Submenu.new({
        text: t('adj.add'),
        enabled: hasDoc,
        items: await Promise.all(ADJUSTMENT_KINDS.map((k) => item(t(`adj.${k}` as Key), () => act.adjust.add(k)))),
      }),
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
  const hasSel = !!app.active?.info.hasSelection;
  const selectMenu = await Submenu.new({
    text: t('menu.select'),
    items: [
      await item(t('select.all'), act.select.all, 'CmdOrCtrl+A', hasDoc),
      await item(t('select.deselect'), act.select.deselect, 'CmdOrCtrl+D', hasSel),
      await item(t('select.reselect'), act.select.reselect, 'CmdOrCtrl+Shift+D', !!app.active?.info.hasLastSelection),
      await item(t('select.inverse'), act.select.invert, 'CmdOrCtrl+Shift+I', hasDoc),
      await sep(),
      await Submenu.new({
        text: t('select.modify'),
        enabled: hasSel,
        items: [
          await item(t('select.feather'), () => act.select.ask('feather'), 'Shift+F6', hasSel),
          await item(t('select.expand'), () => act.select.ask('expand'), undefined, hasSel),
          await item(t('select.contract'), () => act.select.ask('contract'), undefined, hasSel),
          await item(t('select.border'), () => act.select.ask('border'), undefined, hasSel),
          await item(t('select.smooth'), () => act.select.ask('smooth'), undefined, hasSel),
        ],
      }),
      await item(t('select.refine'), act.select.refine, 'CmdOrCtrl+Alt+R', hasSel),
    ],
  });
  const layerMask = app.active ? findLayer(app.active.info.layers, app.active.info.active)?.mask : null;
  const maskMenu = await Submenu.new({
    text: t('mask.menu'),
    enabled: hasDoc,
    items: layerMask
      ? [
          await item(layerMask.enabled ? t('mask.disable') : t('mask.enable'), () => act.mask.toggle()),
          await item(t('mask.invert'), act.mask.invert, 'CmdOrCtrl+I'),
          await item(t('mask.apply'), act.mask.apply),
          await item(t('mask.delete'), act.mask.delete),
        ]
      : [
          await item(t('mask.revealAll'), () => act.mask.add('revealAll')),
          await item(t('mask.hideAll'), () => act.mask.add('hideAll')),
          await item(t('mask.revealSelection'), () => act.mask.add('revealSelection'), undefined, hasSel),
          await item(t('mask.hideSelection'), () => act.mask.add('hideSelection'), undefined, hasSel),
        ],
  });
  await layerMenu.append([await sep(), maskMenu]);
  const placeholders = await Promise.all(
    (['menu.image'] as const).map(async (k) =>
      Submenu.new({ text: t(k), items: [await item(t('tool.notYet'), () => {}, undefined, false)] }),
    ),
  );
  const filterMenu = await Submenu.new({
    text: t('menu.filter'),
    items: await Promise.all(
      FILTER_MENU.map(async (g) =>
        Submenu.new({
          text: t(`filter.group.${g.group}` as Key),
          enabled: hasDoc,
          items: await Promise.all(g.kinds.map((k) => item(`${t(`filter.${k}` as Key)}…`, () => act.openFilter(k)))),
        }),
      ),
    ),
  });
  const items = [file, edit, placeholders[0], layerMenu, selectMenu, filterMenu, viewMenu, windowMenu];
  const menu = await Menu.new({ items: isMac ? [appMenu, ...items] : items });
  await menu.setAsAppMenu();
}
