/**
 * Position status vocabulary.
 *
 * Kept out of the page components so the same list drives the create form, the
 * edit form and the table badges (REACT-UI-REUSE-STANDARD C8: status literals
 * must not be duplicated per page).
 */

import type { PositionStatus } from './types';

export const POSITION_STATUSES: PositionStatus[] = ['Draft', 'Open', 'OnHold', 'Closed'];

/** Server-side default for a newly created position (PositionCreate). */
export const DEFAULT_POSITION_STATUS: PositionStatus = 'Open';
