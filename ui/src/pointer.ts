/**
 * Tells a click from the end of a drag.
 *
 * The browser fires `click` and `mouseup` after a drag too. A click in a widget
 * opens editing, and at the end of a drag it would destroy the new selection.
 * So the widgets compare the release point with the press point.
 */

/**
 * The point of the last mouse press: the distance to it tells a click from the
 * end of a drag. One listener serves the document, as for copy.
 */
let pressedAt: { x: number; y: number } | null = null;

export function onPress(event: MouseEvent): void {
  pressedAt = { x: event.clientX, y: event.clientY };
}

/** The mouse moved away from the press point: a drag, not a click. */
export function draggedFrom(event: MouseEvent): boolean {
  if (!pressedAt) return false;
  return Math.abs(event.clientX - pressedAt.x) > 4 || Math.abs(event.clientY - pressedAt.y) > 4;
}
