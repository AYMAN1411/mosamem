// Live previews of layer property changes: the GPU recomposites while the user drags, and the
// engine recomposites once, when the change is committed. Falls back to transient engine updates
// when the layer's position in the tree can't be previewed exactly on the GPU.
import { engine, findLayer, type LayerInfo, type LayerProps } from '../engine/bridge';
import { display } from '../engine/display';
import { app } from './state.svelte';
import { layer as act } from './actions';

let current: { doc: number; layer: number; session: number | null } | null = null;
let starting: Promise<void> | null = null;

async function ensure(doc: number, layer: number) {
  if (current && current.doc === doc && current.layer === layer) return;
  if (starting) await starting;
  if (current && current.doc === doc && current.layer === layer) return;
  starting = (async () => {
    const session = await engine.liveBegin(doc, layer).catch(() => null);
    current = { doc, layer, session };
  })();
  await starting;
  starting = null;
}

function preview(l: LayerInfo, props: LayerProps) {
  const c = display.compositor;
  if (!c || !current?.session) return false;
  const opacity = props.opacity ?? l.opacity;
  const fill = props.fill ?? l.fill;
  c.setLive({ session: current.session, blend: props.blend ?? l.blend, alpha: opacity * fill, visible: props.visible ?? l.visible });
  return true;
}

/** A change in progress (slider drag). */
export async function liveChange(layerId: number, props: LayerProps) {
  const d = app.active;
  if (!d) return;
  await ensure(d.info.id, layerId);
  const l = findLayer(d.info.layers, layerId);
  if (!l || l.kind !== 'pixel' || !preview(l, props)) await act.set(layerId, props, true);
}

/** The final value: recorded in history; the GPU preview stays until the new composite is ready. */
export async function liveCommit(layerId: number, props: LayerProps) {
  const d = app.active;
  if (!d) return;
  const l = findLayer(d.info.layers, layerId);
  if (l && l.kind === 'pixel' && current?.layer === layerId && current.session) preview(l, props);
  await act.set(layerId, props, false);
  const session = current?.session ?? null;
  current = null;
  if (session != null) display.compositor?.endLive(() => void engine.liveEnd(session));
}

/** Any other edit invalidates a pending live session. */
export function liveReset() {
  if (current?.session != null) {
    display.compositor?.cancelLive();
    void engine.liveEnd(current.session);
  }
  current = null;
}
