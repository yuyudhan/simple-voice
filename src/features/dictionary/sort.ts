// FilePath: src/features/dictionary/sort.ts
import type { DictionaryEntry, DictionarySort } from "../../lib/api";

export type SortColumn = "name" | "added";

const collator = new Intl.Collator(undefined, { sensitivity: "base" });

function byName(a: DictionaryEntry, b: DictionaryEntry): number {
    return collator.compare(a.phrase, b.phrase) || a.id - b.id;
}

// An import stamps every entry with the same time; ids keep them in the order they were added.
function byAge(a: DictionaryEntry, b: DictionaryEntry): number {
    return a.createdAt - b.createdAt || a.id - b.id;
}

const COMPARATORS: Record<DictionarySort, (a: DictionaryEntry, b: DictionaryEntry) => number> = {
    name_asc: byName,
    name_desc: (a, b) => byName(b, a),
    oldest: byAge,
    newest: (a, b) => byAge(b, a),
};

export function sortEntries(entries: DictionaryEntry[], sort: DictionarySort): DictionaryEntry[] {
    return [...entries].sort(COMPARATORS[sort]);
}

/** Clicking the active column flips it; another column starts in its natural order. */
export function nextSort(current: DictionarySort, column: SortColumn): DictionarySort {
    if (column === "name") return current === "name_asc" ? "name_desc" : "name_asc";
    return current === "newest" ? "oldest" : "newest";
}

export function sortColumn(sort: DictionarySort): SortColumn {
    return sort === "name_asc" || sort === "name_desc" ? "name" : "added";
}

/** How `sort` reads aloud and in tooltips. */
export const SORT_DESCRIPTIONS: Record<DictionarySort, string> = {
    name_asc: "A to Z",
    name_desc: "Z to A",
    newest: "newest first",
    oldest: "oldest first",
};
