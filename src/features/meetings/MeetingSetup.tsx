// FilePath: src/features/meetings/MeetingSetup.tsx
// What meetings need on this Mac and whether it is there: the voice model (chosen in Settings →
// Transcription), the speaker model that tells voices apart, and the language model behind
// summaries, chat and translation. Downloads happen only when the user presses Download.
import { useShell } from "../../app/ShellContext";
import { useSettings } from "../../app/SettingsContext";
import type { MeetingAssistant, MeetingReadiness } from "../../lib/api";
import { Badge, Button, Segmented, SettingRow, SettingsGroup, Spinner } from "../../ui";
import type { ModelsState } from "../settings/models/useModels";
import { ModelList } from "../settings/models/VoiceModelList";
import "../settings/common.css";
import "../settings/models/models.css";

function Readiness({ blocker }: { blocker: string | null }) {
    return blocker === null ? (
        <Badge tone="success">Ready</Badge>
    ) : (
        <Badge tone="warning" title={blocker}>
            Not ready
        </Badge>
    );
}

interface Props {
    models: ModelsState;
    readiness: MeetingReadiness | null;
    readinessError: string | null;
}

export function MeetingSetup({ models, readiness, readinessError }: Props) {
    const { settings, update } = useSettings();
    const { navigate } = useShell();
    const list = models.models ?? [];
    const speakerModels = list.filter((m) => m.kind === "diarization");
    const translationModels = list.filter((m) => m.kind === "translation");
    const localModels = list.filter((m) => m.kind === "post_processing" && m.provider === "llm");
    const apple = list.find((m) => m.id === "apple-intelligence");

    if (readiness === null) {
        return (
            <SettingsGroup title="Models for meetings">
                <div className="sv-models__loading">
                    {readinessError ? (
                        <p className="sv-inline-error" role="alert">
                            {readinessError}
                        </p>
                    ) : (
                        <Spinner />
                    )}
                </div>
            </SettingsGroup>
        );
    }

    return (
        <SettingsGroup title="Models for meetings">
            <p className="sv-models-section__intro">
                Meetings are recorded, transcribed, split by speaker, summarised and translated on
                this Mac. Models download once from the source shown; nothing you record is sent
                anywhere.
            </p>
            <SettingRow
                title={
                    <span className="meeting-setup__title">
                        Transcription <Readiness blocker={readiness.transcription} />
                    </span>
                }
                description={readiness.transcription ?? "Uses your voice model from Transcription."}
            >
                <Button
                    size="sm"
                    variant="secondary"
                    onClick={() => {
                        navigate("transcription");
                    }}
                >
                    Voice models
                </Button>
            </SettingRow>
            <SettingRow
                title={
                    <span className="meeting-setup__title">
                        Speakers <Readiness blocker={readiness.diarization} />
                    </span>
                }
                description={
                    readiness.diarization ??
                    "Tells voices apart by how they sound, so each line has its speaker."
                }
            />
            <div className="sv-models-section__flush">
                <ModelList state={models} models={speakerModels} label="Speaker model" />
            </div>
            <SettingRow
                title={
                    <span className="meeting-setup__title">
                        Summary and chat <Readiness blocker={readiness.language} />
                    </span>
                }
                description={
                    readiness.language ??
                    "Writes summaries and action items, and answers questions from the transcript."
                }
            >
                <Segmented<MeetingAssistant>
                    label="Language model for meetings"
                    value={settings.meetingAssistant}
                    options={[
                        { value: "local", label: "Local model" },
                        { value: "apple", label: "Apple Intelligence" },
                    ]}
                    onChange={(meetingAssistant) => {
                        void update({ meetingAssistant });
                    }}
                />
            </SettingRow>
            {settings.meetingAssistant === "local" ? (
                <div className="sv-models-section__flush">
                    <ModelList
                        state={models}
                        models={localModels}
                        label="Local language model"
                        activeId={settings.localLlmModel}
                    />
                </div>
            ) : (
                apple && (
                    <p
                        className={`meeting-setup__note ${
                            apple.status === "ready" ? "sv-inline-note" : "sv-inline-warning"
                        }`}
                    >
                        {apple.status === "ready"
                            ? "Apple Intelligence is available and runs on this Mac."
                            : (apple.reason ?? "Apple Intelligence is not available on this Mac.")}
                    </p>
                )
            )}
            <SettingRow
                title={
                    <span className="meeting-setup__title">
                        Translation <Readiness blocker={readiness.translation} />
                    </span>
                }
                description={
                    readiness.translation ??
                    "A dedicated translation model translates the transcript line by line."
                }
            />
            <div className="sv-models-section__flush">
                <ModelList state={models} models={translationModels} label="Translation model" />
            </div>
        </SettingsGroup>
    );
}
