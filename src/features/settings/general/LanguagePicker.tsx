// FilePath: src/features/settings/general/LanguagePicker.tsx
// Allowed dictation languages plus the fallback a transcription is re-run in when detection
// lands outside them. At least one language always stays selected, and the fallback is always
// one of the selected languages.
import { useMemo, useState } from "react";
import { Check, Plus, Search, X } from "lucide-react";
import { useSettings } from "../../../app/SettingsContext";
import { Select } from "../../../ui";
import { LANGUAGE_NAMES, PRIMARY_LANGUAGES } from "./languages";
import "../common.css";
import "./general.css";

const nameOf = (code: string) => LANGUAGE_NAMES[code] ?? code.toUpperCase();

export function LanguagePicker() {
    const { settings, update } = useSettings();
    const [query, setQuery] = useState("");
    const [open, setOpen] = useState(false);
    const selected = settings.languages;

    const setLanguages = (languages: string[]) => {
        const fallbackLanguage = languages.includes(settings.fallbackLanguage)
            ? settings.fallbackLanguage
            : (languages[0] ?? settings.fallbackLanguage);
        void update({ languages, fallbackLanguage });
    };

    const toggle = (code: string) => {
        if (selected.includes(code)) {
            if (selected.length > 1) setLanguages(selected.filter((c) => c !== code));
        } else {
            setLanguages([...selected, code]);
            setQuery("");
        }
    };

    // An empty query lists every unselected language so a click alone opens the choices.
    const matches = useMemo(() => {
        const needle = query.trim().toLowerCase();
        const unselected = Object.entries(LANGUAGE_NAMES).filter(
            ([code]) => !selected.includes(code),
        );
        if (needle === "") return unselected;
        return unselected
            .filter(([code, name]) => name.toLowerCase().includes(needle) || code === needle)
            .slice(0, 8);
    }, [query, selected]);

    const extra = selected.filter((code) => !PRIMARY_LANGUAGES.includes(code));

    return (
        <div className="sv-langs">
            <div className="sv-langs__chips">
                {PRIMARY_LANGUAGES.map((code) => {
                    const on = selected.includes(code);
                    return (
                        <button
                            key={code}
                            type="button"
                            className={on ? "sv-chip is-on" : "sv-chip"}
                            aria-pressed={on}
                            disabled={on && selected.length === 1}
                            onClick={() => {
                                toggle(code);
                            }}
                        >
                            {on ? <Check size={13} /> : <Plus size={13} />}
                            {nameOf(code)}
                        </button>
                    );
                })}
                {extra.map((code) => (
                    <span key={code} className="sv-chip is-on">
                        {nameOf(code)}
                        <button
                            type="button"
                            className="sv-chip__remove"
                            aria-label={`Remove ${nameOf(code)}`}
                            disabled={selected.length === 1}
                            onClick={() => {
                                toggle(code);
                            }}
                        >
                            <X size={12} />
                        </button>
                    </span>
                ))}
            </div>

            <div
                className="sv-langs__search"
                onBlur={(event) => {
                    if (!event.currentTarget.contains(event.relatedTarget)) setOpen(false);
                }}
            >
                <Search size={14} className="sv-langs__search-icon" />
                <input
                    type="search"
                    className="sv-langs__input"
                    placeholder="Add another language…"
                    value={query}
                    aria-label="Search languages"
                    onFocus={() => {
                        setOpen(true);
                    }}
                    onClick={() => {
                        setOpen(true);
                    }}
                    onChange={(event) => {
                        setQuery(event.target.value);
                        setOpen(true);
                    }}
                    onKeyDown={(event) => {
                        const first = matches[0];
                        if (event.key === "Enter" && query.trim() !== "" && first) {
                            toggle(first[0]);
                        }
                        if (event.key === "Escape" && (open || query !== "")) {
                            event.stopPropagation();
                            setQuery("");
                            setOpen(false);
                        }
                    }}
                />
                {open && matches.length > 0 && (
                    <ul
                        className="sv-langs__results"
                        role="listbox"
                        onMouseDown={(event) => {
                            // WebKit does not focus buttons on click; keeping focus on the
                            // input stops the blur from closing the list before the click lands.
                            event.preventDefault();
                        }}
                    >
                        {matches.map(([code, name]) => (
                            <li key={code}>
                                <button
                                    type="button"
                                    className="sv-langs__result"
                                    onClick={() => {
                                        toggle(code);
                                    }}
                                >
                                    <span>{name}</span>
                                    <span className="sv-langs__code">{code}</span>
                                </button>
                            </li>
                        ))}
                    </ul>
                )}
                {open && query.trim() !== "" && matches.length === 0 && (
                    <p className="sv-langs__empty">No other language matches “{query.trim()}”.</p>
                )}
            </div>

            <div className="sv-langs__fallback">
                <Select
                    label="Fallback language"
                    value={settings.fallbackLanguage}
                    options={selected.map((code) => ({ value: code, label: nameOf(code) }))}
                    onChange={(fallbackLanguage) => {
                        void update({ fallbackLanguage });
                    }}
                />
                <p className="sv-inline-note">
                    Speech detected outside these languages is transcribed again in the fallback
                    language. Hinglish (Hindi mixed with English) stays in Roman script; pure Hindi
                    is written in Devanagari.
                </p>
            </div>
        </div>
    );
}
