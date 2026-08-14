export function selectAvailableItem<T extends { id: string }>(
  filteredItems: T[],
  selectedId: string,
): T | null {
  return filteredItems.find((item) => item.id === selectedId)
    ?? filteredItems[0]
    ?? null;
}

export function completedSuccessfully(result: { phase: string } | null): boolean {
  return result?.phase === "completed";
}

export function toggleAllSelectableIds<T extends { id: string; is_critical?: boolean }>(
  items: T[],
  selectedIds: ReadonlySet<string>,
): Set<string> {
  const selectableIds = items.filter((item) => !item.is_critical).map((item) => item.id);
  const allSelected = selectableIds.length > 0
    && selectableIds.every((id) => selectedIds.has(id));
  return allSelected ? new Set() : new Set(selectableIds);
}
