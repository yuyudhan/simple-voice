// FilePath: src/features/settings/dictation/ContextGroup.tsx
// Settings → Dictation → Context: whether a dictation may read the focused field's text or an
// IDE's visible file and symbol names to spell names right, and which apps are never read. Both
// are off by default, read through Accessibility only when a dictation starts, stay on this Mac,
// and are never kept in History.
import { useCallback, useEffect, useState } from "react";
import { Plus, X } from "lucide-react";
import { api, errorMessage, type KnownApp } from "../../../lib/api";
import { bundleIdError } from "../../../lib/bundleId";
import { useSettings } from "../../../app/SettingsContext";
import {
    Badge,
    Button,
    IconButton,
    Select,
    SettingRow,
    SettingsGroup,
    TextField,
    Toggle,
} from "../../../ui";
import { usePermissions } from "../permissions/usePermissions";
import "../common.css";
import "./dictation.css";

const FOREGROUND_TITLE = "Use text around the cursor";
const FOREGROUND_DESCRIPTION =
    "When a dictation starts, read a short stretch of the focused field and the names in it so " +
    "the voice and formatting models spell them right. Password and secure fields are never " +
    "read. The text is used once on this Mac and never saved.";

const IDE_TITLE = "Use names from your code editor";
const IDE_DESCRIPTION =
    "In a supported code editor, read the visible file names and identifiers so they are " +
    "written exactly. Nothing is run and no project folders are opened. Other editors simply " +
    "add no context.";

/** Select value for typing a bundle ID by hand instead of picking a recent app. */
const TYPED = "";

function ExcludedApps() {
    const { settings, update } = useSettings();
    const [knownApps, setKnownApps] = useState<KnownApp[]>([]);
    const [picked, setPicked] = useState<string | null>(null);
    const [typed, setTyped] = useState("");
    const [error, setError] = useState<string | null>(null);

    const load = useCallback(
        () =>
            api.listKnownApps().then(setKnownApps, (e: unknown) => {
                setError(errorMessage(e));
            }),
        [],
    );

    useEffect(() => {
        void load();
    }, [load]);

    const excluded = settings.contextExcludedApps;
    const excludedKeys = new Set(excluded.map((id) => id.toLowerCase()));
    const candidates = knownApps.filter((app) => !excludedKeys.has(app.bundleId.toLowerCase()));
    const choice = picked ?? candidates[0]?.bundleId ?? TYPED;
    const nameOf = new Map(knownApps.map((app) => [app.bundleId.toLowerCase(), app.appName]));

    const add = async () => {
        const id = choice === TYPED ? typed.trim() : choice;
        const invalid = choice === TYPED ? bundleIdError(id) : null;
        if (invalid !== null) {
            setError(invalid);
            return;
        }
        if (excludedKeys.has(id.toLowerCase())) {
            setError("That app is already excluded.");
            return;
        }
        setError(null);
        if (await update({ contextExcludedApps: [...excluded, id] })) {
            setTyped("");
            setPicked(null);
        }
    };

    return (
        <div className="sv-context__excluded">
            <span className="sv-context__label">Never read these apps</span>
            {excluded.length === 0 ? (
                <p className="sv-inline-note">No apps are excluded.</p>
            ) : (
                <ul className="sv-context__chips">
                    {excluded.map((id) => (
                        <li key={id} className="sv-context__chip">
                            <span title={id}>{nameOf.get(id.toLowerCase()) ?? id}</span>
                            <IconButton
                                label={`Stop excluding ${nameOf.get(id.toLowerCase()) ?? id}`}
                                icon={<X size={13} />}
                                onClick={() => {
                                    void update({
                                        contextExcludedApps: excluded.filter(
                                            (other) => other !== id,
                                        ),
                                    });
                                }}
                            />
                        </li>
                    ))}
                </ul>
            )}
            <form
                className="sv-context__add"
                onSubmit={(event) => {
                    event.preventDefault();
                    void add();
                }}
            >
                <Select
                    ariaLabel="App to exclude"
                    value={choice}
                    options={[
                        ...candidates.map((app) => ({ value: app.bundleId, label: app.appName })),
                        { value: TYPED, label: "Another app (bundle ID)…" },
                    ]}
                    onChange={(value) => {
                        setPicked(value);
                        setError(null);
                    }}
                />
                {choice === TYPED && (
                    <TextField
                        aria-label="Bundle ID to exclude"
                        placeholder="com.example.app"
                        value={typed}
                        spellCheck={false}
                        autoComplete="off"
                        onChange={(event) => {
                            setTyped(event.target.value);
                        }}
                    />
                )}
                <Button type="submit" size="sm" variant="secondary" icon={<Plus size={14} />}>
                    Exclude
                </Button>
            </form>
            {error && (
                <p className="sv-inline-error" role="alert">
                    {error}
                </p>
            )}
        </div>
    );
}

export function ContextGroup() {
    const { settings, update } = useSettings();
    const { permissions } = usePermissions();
    const accessibility = permissions?.accessibility;
    const needsAccess =
        (settings.foregroundContext || settings.ideContext) &&
        accessibility !== undefined &&
        accessibility !== "granted";

    return (
        <SettingsGroup title="Context">
            <SettingRow title={FOREGROUND_TITLE} description={FOREGROUND_DESCRIPTION}>
                <Toggle
                    label={FOREGROUND_TITLE}
                    checked={settings.foregroundContext}
                    onChange={(foregroundContext) => {
                        void update({ foregroundContext });
                    }}
                />
            </SettingRow>
            <SettingRow
                title={
                    <span className="sv-context__title">
                        {IDE_TITLE}
                        <Badge tone="neutral">Code apps</Badge>
                    </span>
                }
                description={IDE_DESCRIPTION}
            >
                <Toggle
                    label={IDE_TITLE}
                    checked={settings.ideContext}
                    onChange={(ideContext) => {
                        void update({ ideContext });
                    }}
                />
            </SettingRow>
            {(settings.foregroundContext || settings.ideContext) &&
                (settings.transcriptionProvider === "groq" ||
                    settings.postProcessing === "groq") && (
                    <p className="sv-inline-warning sv-context__warning" role="status">
                        Groq is chosen for transcription or formatting, so no context is read: text
                        from other apps never leaves this Mac. Switch both back to on-device models
                        to use context.
                    </p>
                )}
            {needsAccess && (
                <p className="sv-inline-warning sv-context__warning" role="status">
                    Accessibility access is off, so no context is read and dictation works as
                    before. Grant it in Privacy &amp; data to use context.
                </p>
            )}
            <ExcludedApps />
        </SettingsGroup>
    );
}
