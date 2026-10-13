// FilePath: src/features/style/StyleOverrides.tsx
// Style per kind of app and per app. When a dictation starts, the app's own style wins, then the
// style of its kind (the same categories as History and Insights), then the global style.
import { useCallback, useEffect, useState, type SyntheticEvent } from "react";
import { Plus, Trash2 } from "lucide-react";
import {
    api,
    errorMessage,
    type AppCategory,
    type AppStyleOverride,
    type KnownApp,
    type Style,
} from "../../lib/api";
import { bundleIdError } from "../../lib/bundleId";
import { CATEGORY_META } from "../../lib/categories";
import { useSettings } from "../../app/SettingsContext";
import {
    Button,
    IconButton,
    Select,
    SettingRow,
    SettingsGroup,
    TextField,
    type SelectOption,
} from "../../ui";
import "../settings/common.css";

const STYLE_LABEL: Record<Style, string> = { formal: "Formal", casual: "Casual" };

/** In the order History and Insights list them. */
const CATEGORIES: readonly AppCategory[] = [
    "work_messages",
    "personal_messages",
    "email",
    "documents",
    "ai_prompts",
    "code",
    "other",
];

type CategoryChoice = Style | "global";

/** Select value for picking a recently used app, or typing a bundle ID by hand. */
const OTHER_APP = "";

function CategoryStyles() {
    const { settings, update } = useSettings();
    const options: SelectOption<CategoryChoice>[] = [
        { value: "global", label: `Global (${STYLE_LABEL[settings.style]})` },
        { value: "formal", label: "Formal" },
        { value: "casual", label: "Casual" },
    ];

    const choose = (category: AppCategory, choice: CategoryChoice) => {
        const next: Partial<Record<AppCategory, Style>> = {};
        for (const key of CATEGORIES) {
            const value = key === category ? choice : settings.categoryStyles[key];
            if (value !== undefined && value !== "global") next[key] = value;
        }
        void update({ categoryStyles: next });
    };

    return (
        <SettingsGroup title="By kind of app">
            {CATEGORIES.map((category) => (
                <SettingRow
                    key={category}
                    title={
                        <span className="style-category">
                            <span className="style-category__icon" aria-hidden="true">
                                {CATEGORY_META[category].icon}
                            </span>
                            {CATEGORY_META[category].label}
                        </span>
                    }
                >
                    <Select
                        ariaLabel={`${CATEGORY_META[category].label} style`}
                        value={settings.categoryStyles[category] ?? "global"}
                        options={options}
                        onChange={(choice) => {
                            choose(category, choice);
                        }}
                    />
                </SettingRow>
            ))}
        </SettingsGroup>
    );
}

interface AddFormProps {
    knownApps: KnownApp[];
    existing: AppStyleOverride[];
    onAdd: (override: AppStyleOverride) => Promise<boolean>;
}

function AddOverride({ knownApps, existing, onAdd }: AddFormProps) {
    const taken = new Set(existing.map((o) => o.bundleId.toLowerCase()));
    const available = knownApps.filter((app) => !taken.has(app.bundleId.toLowerCase()));
    // null = not chosen yet: the first recent app without its own style, once the list loads.
    const [chosen, setChosen] = useState<string | null>(null);
    const picked = chosen ?? available[0]?.bundleId ?? OTHER_APP;
    const [bundleId, setBundleId] = useState("");
    const [appName, setAppName] = useState("");
    const [style, setStyle] = useState<Style>("casual");
    const [error, setError] = useState<string | null>(null);
    const [saving, setSaving] = useState(false);

    // A picked app leaves the list once it gets an override.
    const pickedApp = available.find((app) => app.bundleId === picked);
    const typing = pickedApp === undefined;

    const appOptions: SelectOption<string>[] = [
        ...available.map((app) => ({
            value: app.bundleId,
            label: `${app.appName} · ${CATEGORY_META[app.category].label}`,
        })),
        { value: OTHER_APP, label: "Another app (bundle ID)…" },
    ];

    async function submit(event: SyntheticEvent) {
        event.preventDefault();
        const id = typing ? bundleId.trim() : pickedApp.bundleId;
        const typedName = appName.trim();
        const name = typing ? (typedName === "" ? id : typedName) : pickedApp.appName;
        const invalid = typing ? bundleIdError(id) : null;
        if (invalid !== null) {
            setError(invalid);
            return;
        }
        if (taken.has(id.toLowerCase())) {
            setError("That app already has its own style.");
            return;
        }
        setSaving(true);
        setError(null);
        const added = await onAdd({ bundleId: id, appName: name, style });
        setSaving(false);
        if (added) {
            setBundleId("");
            setAppName("");
            setChosen(null);
        }
    }

    return (
        <form
            className="style-add"
            onSubmit={(event) => {
                void submit(event);
            }}
        >
            <div className="style-add__row">
                <Select
                    label="App"
                    value={typing ? OTHER_APP : picked}
                    options={appOptions}
                    onChange={(value) => {
                        setChosen(value);
                        setError(null);
                    }}
                />
                <Select
                    label="Style"
                    value={style}
                    options={[
                        { value: "formal", label: "Formal" },
                        { value: "casual", label: "Casual" },
                    ]}
                    onChange={setStyle}
                />
                <Button type="submit" variant="secondary" icon={<Plus />} loading={saving}>
                    Add
                </Button>
            </div>
            {typing && (
                <div className="style-add__row">
                    <TextField
                        label="Bundle ID"
                        error={error ?? undefined}
                        placeholder="com.example.app"
                        value={bundleId}
                        spellCheck={false}
                        autoComplete="off"
                        onChange={(event) => {
                            setBundleId(event.target.value);
                        }}
                    />
                    <TextField
                        label="Name (optional)"
                        placeholder="Example"
                        value={appName}
                        onChange={(event) => {
                            setAppName(event.target.value);
                        }}
                    />
                </div>
            )}
            {error && !typing && (
                <p className="sv-inline-error" role="alert">
                    {error}
                </p>
            )}
        </form>
    );
}

function AppStyles() {
    const { settings, update } = useSettings();
    const [knownApps, setKnownApps] = useState<KnownApp[]>([]);
    const [loadError, setLoadError] = useState<string | null>(null);

    const load = useCallback(
        () =>
            api.listKnownApps().then(
                (apps) => {
                    setKnownApps(apps);
                    setLoadError(null);
                },
                (error: unknown) => {
                    setLoadError(errorMessage(error));
                },
            ),
        [],
    );

    // The frontmost app changes while Simple Voice is in the background; re-read on focus.
    useEffect(() => {
        void load();
        const onFocus = () => {
            void load();
        };
        window.addEventListener("focus", onFocus);
        return () => {
            window.removeEventListener("focus", onFocus);
        };
    }, [load]);

    const setOverrides = (appStyles: AppStyleOverride[]) => update({ appStyles });

    return (
        <SettingsGroup title="By app">
            {settings.appStyles.map((override) => {
                const bundleKey = override.bundleId.toLowerCase();
                const category = knownApps.find(
                    (app) => app.bundleId.toLowerCase() === bundleKey,
                )?.category;
                return (
                    <SettingRow
                        key={override.bundleId}
                        title={override.appName}
                        description={
                            <span className="style-app__id">
                                {override.bundleId}
                                {category && ` · overrides ${CATEGORY_META[category].label}`}
                            </span>
                        }
                    >
                        <div className="style-app__control">
                            <Select
                                ariaLabel={`${override.appName} style`}
                                value={override.style}
                                options={[
                                    { value: "formal", label: "Formal" },
                                    { value: "casual", label: "Casual" },
                                ]}
                                onChange={(style) => {
                                    void setOverrides(
                                        settings.appStyles.map((o) =>
                                            o.bundleId === override.bundleId ? { ...o, style } : o,
                                        ),
                                    );
                                }}
                            />
                            <IconButton
                                label={`Remove the style for ${override.appName}`}
                                icon={<Trash2 />}
                                onClick={() => {
                                    void setOverrides(
                                        settings.appStyles.filter(
                                            (o) => o.bundleId !== override.bundleId,
                                        ),
                                    );
                                }}
                            />
                        </div>
                    </SettingRow>
                );
            })}
            <div className="style-add__wrap">
                <AddOverride
                    knownApps={knownApps}
                    existing={settings.appStyles}
                    onAdd={(override) => setOverrides([...settings.appStyles, override])}
                />
                {loadError && (
                    <p className="sv-inline-error" role="alert">
                        Could not list recent apps: {loadError}
                    </p>
                )}
            </div>
        </SettingsGroup>
    );
}

export function StyleOverrides() {
    return (
        <div className="style-overrides">
            <p className="style-overrides__note">
                An app’s own style wins over its kind, and its kind wins over the global style. The
                style is chosen when a dictation starts; History shows which one was used.
            </p>
            <CategoryStyles />
            <AppStyles />
        </div>
    );
}
