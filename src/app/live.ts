// Live previews of layer property changes: the GPU recomposites while the user drags, and the
// engine recomposites once, when the change is committed. Falls back to transient engine updates
// when the layer's position in the tree can't be previewed exactly on the GPU.
//
// Adjustment layers preview the same way: the GPU applies the adjustment (compiled by the engine,
// so it runs on exactly the engine's numbers) to everything below the layer.
import { engine, findLayer, type LayerInfo, type LayerProps } from '../engine/bridge';
import type { Adjustment } from '../engine/adjust';
import { decodeCompiledAdjustment, type CompiledAdjustment } from '../engine/compositor';
import { display } from '../engine/display';
import { app } from './state.svelte';
import { adjust as adjustAct, layer as act } from './actions';

let current: { doc: number; layer: number; session: number | null; adjust: CompiledAdjustment | null } | null = null;
let starting: Promise<void> | null = null;

async function ensure(doc: number, layer: number) {
  if (current && current.doc === doc && current.layer === layer) return;
  if (starting) await starting;
  if (current && current.doc === doc && current.layer === layer) return;
  starting = (async () => {
    const session = await engine.liveBegin(doc, layer).catch(() => null);
    current = { doc, layer, session, adjust: null };
  })();
  await starting;
  starting = null;
}

const compile = async (a: Adjustment) => decodeCompiledAdjustment(await engine.adjustmentCompile(a));
const canPreview = (l: LayerInfo) => l.kind === 'pixel' || (l.kind === 'adjustment' && !!l.adjustment);

function preview(l: LayerInfo, props: LayerProps) {
  const c = display.compositor;
  if (!c || !current?.session) return false;
  if (l.kind === 'adjustment' && !current.adjust) return false;
  const opacity = props.opacity ?? l.opacity;
  const fill = props.fill ?? l.fill;
  c.setLive({ session: current.session, blend: props.blend ?? l.blend, alpha: opacity * fill, visible: props.visible ?? l.visible, adjust: current.adjust });
  return true;
}

/** A change in progress (slider drag). */
export async function liveChange(layerId: number, props: LayerProps) {
  const d = app.active;
  if (!d) return;
  await ensure(d.info.id, layerId);
  const l = findLayer(d.info.layers, layerId);
  if (l?.kind === 'adjustment' && current?.session && !current.adjust) current.adjust = await compile(l.adjustment!);
  if (!l || !canPreview(l) || !preview(l, props)) await act.set(layerId, props, true);
}

/** The final value: recorded in history; the GPU preview stays until the new composite is ready. */
export async function liveCommit(layerId: number, props: LayerProps) {
  const d = app.active;
  if (!d) return;
  const l = findLayer(d.info.layers, layerId);
  if (l && canPreview(l) && current?.layer === layerId && current.session) preview(l, props);
  await act.set(layerId, props, false);
  finish();
}

let adjustSeq = 0;

/** Adjustment settings being dragged. Returns false when the GPU can't preview this layer. */
export async function liveAdjust(layerId: number, settings: Adjustment): Promise<boolean> {
  const d = app.active;
  if (!d) return false;
  const seq = ++adjustSeq;
  await ensure(d.info.id, layerId);
  if (!current?.session) return false;
  const op = await compile(settings);
  // a newer drag position already arrived: let it win
  if (seq !== adjustSeq || current?.layer !== layerId) return true;
  current.adjust = op;
  const l = findLayer(app.active?.info.layers ?? [], layerId);
  return !!l && preview(l, {});
}

/** The released adjustment settings: one history step, then back to the engine's composite. */
export async function liveAdjustCommit(layerId: number, settings: Adjustment) {
  adjustSeq++;
  await adjustAct.commit(layerId, settings);
  finish();
}

function finish() {
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
