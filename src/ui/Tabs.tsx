// FilePath: src/ui/Tabs.tsx
import { useRef, type KeyboardEvent, type ReactNode } from "react";
import "./Tabs.css";

export interface TabItem {
    id: string;
    label: string;
    badge?: ReactNode;
}

export interface TabsProps {
    items: TabItem[];
    value: string;
    onChange: (id: string) => void;
}

export function Tabs({ items, value, onChange }: TabsProps) {
    const listRef = useRef<HTMLDivElement>(null);

    // Arrow keys move between tabs, matching the WAI-ARIA tabs pattern.
    function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
        const index = items.findIndex((item) => item.id === value);
        let next: number;
        if (event.key === "ArrowRight" || event.key === "ArrowDown") next = index + 1;
        else if (event.key === "ArrowLeft" || event.key === "ArrowUp") next = index - 1;
        else if (event.key === "Home") next = 0;
        else if (event.key === "End") next = items.length - 1;
        else return;
        event.preventDefault();
        const wrapped = (next + items.length) % items.length;
        const target = items[wrapped];
        if (!target) return;
        onChange(target.id);
        const buttons = listRef.current?.querySelectorAll<HTMLButtonElement>("[role=tab]");
        buttons?.[wrapped]?.focus();
    }

    return (
        <div className="sv-tabs" role="tablist" ref={listRef} onKeyDown={onKeyDown}>
            {items.map((item) => {
                const selected = item.id === value;
                return (
                    <button
                        key={item.id}
                        type="button"
                        role="tab"
                        aria-selected={selected}
                        tabIndex={selected ? 0 : -1}
                        className={`sv-tabs__tab${selected ? " is-selected" : ""}`}
                        onClick={() => {
                            onChange(item.id);
                        }}
                    >
                        <span>{item.label}</span>
                        {item.badge !== undefined && item.badge !== null && (
                            <span className="sv-tabs__badge">{item.badge}</span>
                        )}
                    </button>
                );
            })}
        </div>
    );
}
