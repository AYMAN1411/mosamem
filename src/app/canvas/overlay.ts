// Canvas overlay drawing: document edge, grid and guides, in viewport CSS px.
import { docToViewport, type DocState, type Guide } from '../state.svelte';

interface OverlayOptions {
  grid: { spacing: number; subdivisions: number } | null;
  guides: Guide[];
  hot: Guide | null;
}

function line(ctx: CanvasRenderingContext2D, d: DocState, vp: { w: number; h: number }, x0: number, y0: number, x1: number, y1: number) {
  const a = docToViewport(d.view, vp, x0, y0);
  const b = docToViewport(d.view, vp, x1, y1);
  ctx.moveTo(Math.round(a.x) + 0.5, Math.round(a.y) + 0.5);
  ctx.lineTo(Math.round(b.x) + 0.5, Math.round(b.y) + 0.5);
}

export function drawOverlay(ctx: CanvasRenderingContext2D, d: DocState, vp: { w: number; h: number }, o: OverlayOptions) {
  const { width: W, height: H } = d.info;
  const z = d.view.zoom;

  // document edge
  ctx.beginPath();
  ctx.strokeStyle = 'rgba(0,0,0,0.55)';
  ctx.lineWidth = 1;
  line(ctx, d, vp, 0, 0, W, 0);
  line(ctx, d, vp, W, 0, W, H);
  line(ctx, d, vp, W, H, 0, H);
  line(ctx, d, vp, 0, H, 0, 0);
  ctx.stroke();

  if (o.grid) {
    const major = o.grid.spacing;
    const minor = major / Math.max(1, o.grid.subdivisions);
    for (const [step, style] of [
      [minor, 'rgba(128,128,128,0.22)'],
      [major, 'rgba(128,128,128,0.55)'],
    ] as const) {
      if (step * z < 4) continue; // too dense to be useful
      ctx.beginPath();
      ctx.strokeStyle = style;
      for (let x = step; x < W; x += step) line(ctx, d, vp, x, 0, x, H);
      for (let y = step; y < H; y += step) line(ctx, d, vp, 0, y, W, y);
      ctx.stroke();
    }
  }

  const far = 1e6;
  for (const g of o.guides) {
    ctx.beginPath();
    ctx.strokeStyle = g === o.hot || g.id === o.hot?.id ? '#ffffff' : getComputedStyle(document.documentElement).getPropertyValue('--guide').trim() || '#3fd2ff';
    if (g.axis === 'h') line(ctx, d, vp, -far, g.pos, far, g.pos);
    else line(ctx, d, vp, g.pos, -far, g.pos, far);
    ctx.stroke();
  }
}
