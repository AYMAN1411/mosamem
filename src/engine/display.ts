// The canvas's compositor, shared with code outside the canvas view (live previews, tests).
import type { Compositor } from './compositor';

export const display: { compositor: Compositor | null } = { compositor: null };
