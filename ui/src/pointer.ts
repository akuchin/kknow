let pressedAt: { x: number; y: number } | null = null;

export function onPress(event: MouseEvent): void {
  pressedAt = { x: event.clientX, y: event.clientY };
}

export function draggedFrom(event: MouseEvent): boolean {
  if (!pressedAt) return false;
  return Math.abs(event.clientX - pressedAt.x) > 4 || Math.abs(event.clientY - pressedAt.y) > 4;
}
