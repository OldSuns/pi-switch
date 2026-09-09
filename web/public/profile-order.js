import { modelName } from "./ui.js";

const names = new Intl.Collator("zh-CN", { sensitivity: "base", numeric: true });

export function orderedItems(items, ordering) {
  const ranks = new Map(ordering.order.map((id, index) => [id, index]));
  const byPosition = (left, right) => ranks.get(left.id) - ranks.get(right.id);
  if (ordering.sort === "custom") return [...items].sort(byPosition);
  const direction = ordering.sort.endsWith("-desc") ? -1 : 1;
  return [...items].sort((left, right) => {
    let compared;
    if (ordering.sort.startsWith("name-")) {
      compared = names.compare(modelName(left), modelName(right));
    } else {
      const leftDate = ordering.addedAt[left.id];
      const rightDate = ordering.addedAt[right.id];
      if (leftDate == null || rightDate == null) {
        if (leftDate == null && rightDate == null) return byPosition(left, right);
        return leftDate == null ? 1 : -1;
      }
      compared = Date.parse(leftDate) - Date.parse(rightDate);
    }
    return compared * direction || byPosition(left, right);
  });
}

export function orderSnapshot(snapshot) {
  return {
    ...snapshot,
    providers: orderedItems(snapshot.providers, snapshot.ordering.providers).map((provider) => ({
      ...provider, models: orderedItems(provider.models, snapshot.ordering.models[provider.id]),
    })),
  };
}

export function reorderedVisibleIds(items, visible, sourceId, insertionIndex) {
  const visibleIds = visible.map((item) => item.id);
  const sourceIndex = visibleIds.indexOf(sourceId);
  if (sourceIndex < 0) throw new Error("The item is no longer in the visible list.");
  if (!Number.isInteger(insertionIndex) || insertionIndex < 0 || insertionIndex > visibleIds.length) throw new Error("Invalid list insertion position.");
  const reordered = visibleIds.filter((id) => id !== sourceId);
  reordered.splice(insertionIndex - Number(insertionIndex > sourceIndex), 0, sourceId);
  const visibleSet = new Set(visibleIds);
  let index = 0;
  // Filtered-out entries keep their slots when the visible subset is moved.
  return items.map((item) => visibleSet.has(item.id) ? reordered[index++] : item.id);
}
