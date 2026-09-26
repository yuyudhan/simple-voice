// FilePath: src/features/dictionary/SortHeader.tsx
import { ArrowDown, ArrowUp } from "lucide-react";
import type { DictionarySort } from "../../lib/api";
import { nextSort, SORT_DESCRIPTIONS, sortColumn, type SortColumn } from "./sort";

export interface SortHeaderProps {
    label: string;
    column: SortColumn;
    sort: DictionarySort;
    onSort: (sort: DictionarySort) => void;
}

/** A column head that sorts the list; the active one carries an arrow for its direction. */
export function SortHeader({ label, column, sort, onSort }: SortHeaderProps) {
    const active = sortColumn(sort) === column;
    const next = nextSort(sort, column);
    const ascending = sort === "name_asc" || sort === "oldest";
    const Arrow = ascending ? ArrowUp : ArrowDown;
    return (
        <button
            type="button"
            className={`dictionary-sort caps-label${active ? " is-active" : ""}`}
            aria-pressed={active}
            title={`Sort ${SORT_DESCRIPTIONS[next]}`}
            aria-label={
                active
                    ? `${label}, sorted ${SORT_DESCRIPTIONS[sort]}. Sort ${SORT_DESCRIPTIONS[next]}`
                    : `Sort by ${label.toLowerCase()}, ${SORT_DESCRIPTIONS[next]}`
            }
            onClick={() => {
                onSort(next);
            }}
        >
            {label}
            {active && <Arrow aria-hidden="true" />}
        </button>
    );
}
