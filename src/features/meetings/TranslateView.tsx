// FilePath: src/features/meetings/TranslateView.tsx
// The transcript translated on this Mac by the dedicated translation model into a language the
// user picks, line by line above the original. Every translation made is kept with the meeting.
// The model is separate from the summary/chat model: until it is downloaded, the tab offers its
// download instead of translating.
import { useState } from "react";
import { Languages } from "lucide-react";
import { useSettings } from "../../app/SettingsContext";
import { api, type MeetingDetail } from "../../lib/api";
import { languageOf, useLanguages } from "../../lib/useLanguages";
import { Button, EmptyState, Select, Spinner } from "../../ui";
import { useModels } from "../settings/models/useModels";
import { ModelList } from "../settings/models/VoiceModelList";
import { JobProgress } from "./JobProgress";
import { formatClock } from "./useMeetings";
import { useMeetingJob } from "./useMeetingJob";

interface Props {
    detail: MeetingDetail;
    /** Why the translation model can't run; null when it can. */
    blocker: string | null;
    onChanged: () => void;
}

export function TranslateView({ detail, blocker, onChanged }: Props) {
    const { settings, update } = useSettings();
    const { languages, loadError } = useLanguages();
    const models = useModels();
    const { meeting, translations } = detail;
    const job = useMeetingJob(meeting.id, "translate");
    const [target, setTarget] = useState(settings.translationLanguage);
    // Which stored translation is shown; null shows the newest.
    const [shown, setShown] = useState<string | null>(null);
    const translators = (models.models ?? []).filter((m) => m.kind === "translation");

    let why: string | null;
    if (meeting.status !== "ready") why = "The transcript can be translated once it is ready.";
    else if (detail.segments.length === 0) why = "There is no transcript to translate.";
    else why = blocker;

    if (languages === null) {
        return (
            <div className="meeting-loading">
                {loadError ? (
                    <p className="sv-inline-error" role="alert">
                        {loadError}
                    </p>
                ) : (
                    <Spinner />
                )}
            </div>
        );
    }

    const translation =
        translations.find((t) => t.language === shown) ?? translations[translations.length - 1];
    const byId = new Map(translation?.segments.map((s) => [s.segmentId, s.text]) ?? []);
    // The model writes each language in its own script only, so romanised variants (Hinglish)
    // are not offered.
    const options = languages
        .filter((language) => language.fallback)
        .map((language) => ({
            value: language.tag,
            label: language.script ? `${language.name} (${language.script})` : language.name,
        }));
    // A saved choice the model can't write (e.g. Hinglish) falls back to the first offered one.
    const chosen = options.some((option) => option.value === target)
        ? target
        : (options[0]?.value ?? target);

    async function translate() {
        if (chosen !== settings.translationLanguage) {
            void update({ translationLanguage: chosen });
        }
        const made = await job.run(() => api.translateMeeting(meeting.id, chosen));
        if (made !== null) {
            setShown(made.language);
            onChanged();
        }
    }

    return (
        <div className="meeting-translate">
            <div className="meeting-translate__bar">
                <Select
                    label="Translate into"
                    value={chosen}
                    options={options}
                    disabled={job.running}
                    onChange={setTarget}
                />
                <Button
                    variant="primary"
                    icon={<Languages />}
                    disabled={why !== null || job.running}
                    onClick={() => {
                        void translate();
                    }}
                >
                    Translate
                </Button>
                {translations.length > 1 && translation && (
                    <Select
                        label="Showing"
                        value={translation.language}
                        options={translations.map((t) => ({
                            value: t.language,
                            label: languageOf(languages, t.language).name,
                        }))}
                        onChange={setShown}
                    />
                )}
            </div>
            {why !== null && <p className="sv-inline-warning">{why}</p>}
            {blocker !== null && translators.length > 0 && (
                <ModelList state={models} models={translators} label="Translation model" />
            )}
            <JobProgress job={job} label="Translating on this Mac…" />
            {job.error && (
                <p className="sv-inline-error" role="alert">
                    {job.error}
                </p>
            )}
            {translation === undefined ? (
                !job.running && (
                    <EmptyState
                        icon={<Languages />}
                        title="Not translated yet"
                        description="Pick a language and translate the whole transcript on this Mac."
                    />
                )
            ) : (
                <>
                    <p className="meeting-summary__meta">
                        {languageOf(languages, translation.language).name} · translated by{" "}
                        {translators.find((m) => m.id === translation.model)?.name ??
                            translation.model}{" "}
                        on this Mac
                    </p>
                    <p className="sv-inline-warning" role="note">
                        Machine translation, unverified: check dates and names against the original
                        under each line.
                    </p>
                    <ol className="meeting-lines">
                        {detail.segments.map((segment) => (
                            <li key={segment.id} className="meeting-line meeting-line--translated">
                                <span className="meeting-line__time">
                                    {formatClock(segment.startMs)}
                                </span>
                                <div className="meeting-line__pair">
                                    <p className="meeting-line__text selectable">
                                        {byId.get(segment.id) ?? "—"}
                                    </p>
                                    <p className="meeting-line__original selectable">
                                        {segment.text}
                                    </p>
                                </div>
                            </li>
                        ))}
                    </ol>
                </>
            )}
        </div>
    );
}
