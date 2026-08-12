export function selectAvailableItem<T extends { id: string }>(
  filteredItems: T[],
  allItems: T[],
  selectedId: string,
  previewFallback: T | null,
): T | null {
  return filteredItems.find((item) => item.id === selectedId)
    ?? filteredItems[0]
    ?? allItems[0]
    ?? previewFallback;
}

export function completedSuccessfully(result: { phase: string } | null): boolean {
  return result?.phase === "completed";
}
