// FilePath: src/features/settings/transcription/LanguagePicker.tsx
// Allowed dictation languages plus the fallback a transcription is re-run in when detection
// lands outside them. The languages come from the backend registry, so adding one there adds it
// here. At least one language always stays selected, and the fallback is always one of the
// selected languages that may be a fallback (a romanised variant never is).
import { useMemo, useState } from "react";
import { Check, Plus, Search, X } from "lucide-react";
import { useSettings } from "../../../app/SettingsContext";
import type { LanguageInfo } from "../../../lib/api";
import { languageOf, useLanguages } from "../../../lib/useLanguages";
import { Select, Spinner } from "../../../ui";
import "../common.css";
import "./languages.css";

const FALLBACK_HINT = "Speech in any other language is transcribed as the fallback language.";

/** Name plus, when the script is what tells it apart from a sibling, the script muted. */
function LanguageLabel({ language }: { language: LanguageInfo }) {
    return (
        <>
            {language.name}
            {language.script !== null && (
                <span className="sv-chip__script">· {language.script}</span>
            )}
        </>
    );
}

export function LanguagePicker() {
    const { languages, loadError } = useLanguages();
    if (languages === null) {
        return (
            <div className="sv-langs">
                {loadError ? (
                    <p className="sv-model__error" role="alert">
                        {loadError}
                    </p>
                ) : (
                    <Spinner />
                )}
            </div>
        );
    }
    return <LanguageChoices registry={languages} />;
}

function LanguageChoices({ registry }: { registry: LanguageInfo[] }) {
    const { settings, update } = useSettings();
    const [query, setQuery] = useState("");
    const [open, setOpen] = useState(false);
    const selected = settings.languages;
    const info = (tag: string) => languageOf(registry, tag);
    const fallbackChoices = selected.filter((tag) => info(tag).fallback);

    const setLanguages = (languages: string[]) => {
        const choices = languages.filter((tag) => info(tag).fallback);
        // With no selected language able to be the fallback, the stored one stays as it is.
        const fallbackLanguage = choices.includes(settings.fallbackLanguage)
            ? settings.fallbackLanguage
            : (choices[0] ?? settings.fallbackLanguage);
        void update({ languages, fallbackLanguage });
    };

    const toggle = (tag: string) => {
        if (selected.includes(tag)) {
            if (selected.length > 1) setLanguages(selected.filter((t) => t !== tag));
        } else {
            setLanguages([...selected, tag]);
            setQuery("");
        }
    };

    const typed = query.trim();

    // An empty query lists every unselected language so a click alone opens the choices.
    const matches = useMemo(() => {
        const needle = query.trim().toLowerCase();
        const unselected = registry.filter((language) => !selected.includes(language.tag));
        if (needle === "") return unselected;
        return unselected
            .filter(
                (language) =>
                    language.name.toLowerCase().includes(needle) ||
                    language.tag.toLowerCase() === needle ||
                    (language.script?.toLowerCase().includes(needle) ?? false),
            )
            .slice(0, 8);
    }, [query, selected, registry]);

    const primary = registry.filter((language) => language.primary);
    const extra = selected.filter((tag) => !primary.some((language) => language.tag === tag));
    const fallbackOptions =
        fallbackChoices.length > 0 ? fallbackChoices : [settings.fallbackLanguage];

    return (
        <div className="sv-langs">
            <div className="sv-langs__chips">
                {primary.map((language) => {
                    const on = selected.includes(language.tag);
                    return (
                        <button
                            key={language.tag}
                            type="button"
                            className={on ? "sv-chip is-on" : "sv-chip"}
                            aria-pressed={on}
                            disabled={on && selected.length === 1}
                            onClick={() => {
                                toggle(language.tag);
                            }}
                        >
                            {on ? <Check size={13} /> : <Plus size={13} />}
                            <LanguageLabel language={language} />
                        </button>
                    );
                })}
                {extra.map((tag) => (
                    <span key={tag} className="sv-chip is-on">
                        <LanguageLabel language={info(tag)} />
                        <button
                            type="button"
                            className="sv-chip__remove"
                            aria-label={`Remove ${info(tag).name}`}
                            disabled={selected.length === 1}
                            onClick={() => {
                                toggle(tag);
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
                            toggle(first.tag);
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
                        {matches.map((language) => (
                            <li key={language.tag}>
                                <button
                                    type="button"
                                    className="sv-langs__result"
                                    onClick={() => {
                                        toggle(language.tag);
                                    }}
                                >
                                    <span>
                                        <LanguageLabel language={language} />
                                    </span>
                                    <span className="sv-langs__code">{language.tag}</span>
                                </button>
                            </li>
                        ))}
                    </ul>
                )}
                {open && typed !== "" && matches.length === 0 && (
                    <p className="sv-langs__empty">No other language matches “{typed}”.</p>
                )}
            </div>

            <div className="sv-langs__fallback">
                <Select
                    label="Fallback language"
                    value={settings.fallbackLanguage}
                    options={fallbackOptions.map((tag) => ({
                        value: tag,
                        label: info(tag).name,
                    }))}
                    onChange={(fallbackLanguage) => {
                        void update({ fallbackLanguage });
                    }}
                />
                <p className="sv-inline-note">{FALLBACK_HINT}</p>
            </div>
        </div>
    );
}
