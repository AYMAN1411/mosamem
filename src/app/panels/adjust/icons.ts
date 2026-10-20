import type { AdjustmentKind } from '../../../engine/adjust';
import type { IconName } from '../../icons/icons';

export function adjustmentIcon(kind: AdjustmentKind): IconName {
  return `adj${kind[0].toUpperCase()}${kind.slice(1)}` as IconName;
}
