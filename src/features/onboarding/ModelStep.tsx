// FilePath: src/features/onboarding/ModelStep.tsx
// Voice model choice, with Smart Select (beta) one switch away. A local model leads with its
// download card (a new install: Parakeet TDT v3) and the full list sits one click away.
// Continuing requires a usable model: Groq with a verified key or a downloaded local model (plus
// Speech Recognition access for Apple Speech); for Smart Select, a saved Groq key or every local
// model of its plan downloaded.
import { useState } from "react";
import { AudioLines, ChevronDown, ChevronUp } from "lucide-react";
import { useSettings } from "../../app/SettingsContext";
import { Button } from "../../ui";
import { GroqKeyField } from "../settings/models/GroqKeyField";
import { useModels } from "../settings/models/useModels";
import { VoiceModelList } from "../settings/models/VoiceModelList";
import { PermissionAction, StatusBadge } from "../settings/permissions/PermissionsGroup";
import { usePermissions } from "../settings/permissions/usePermissions";
import { SmartSelectPanel, VoiceModelModeSwitch } from "../settings/transcription/SmartSelectPanel";
import { useSmartSelect } from "../settings/transcription/useSmartSelect";
import { StepFooter } from "./StepFooter";
import { ModelDownloadCard } from "./ModelDownloadCard";

interface Props {
    onBack: () => void;
    onNext: () => void;
}

export function ModelStep({ onBack, onNext }: Props) {
    const { settings } = useSettings();
    const models = useModels();
    const smart = useSmartSelect(models);
    const { permissions, busy, request, openSettings } = usePermissions(1500);
    const isSmart = smart.mode === "smart";
    const active = models.models?.find((m) => m.id === settings.transcriptionModel);
    const isGroq = !isSmart && settings.transcriptionModel === "groq-whisper";
    const isAppleSpeech = !isSmart && active?.provider === "apple";
    const speech = permissions?.speech;
    const [listOpen, setListOpen] = useState(false);
    // The model this step is setting up: one waiting on its download, else the chosen one.
    const target = isSmart
        ? undefined
        : models.models?.find((m) => m.id === (models.pendingUse ?? settings.transcriptionModel));
    const featured =
        target !== undefined && target.provider !== "groq" && target.sizeMb !== null
            ? target
            : undefined;
    const showList = featured === undefined || listOpen;

    let ready = false;
    let blocker = "";
    if (isSmart) {
        const plan = smart.plan;
        ready =
            plan !== null &&
            (plan.groq ||
                (models.models ?? []).some((m) => m.id === plan.local && m.status === "ready"));
        blocker = "Download the model above, or save a Groq API key.";
    } else if (isGroq) {
        ready = settings.groqApiKeyPresent;
        blocker = "Save a Groq API key to continue, or pick a local model.";
    } else if (active === undefined && models.pendingUse === null) {
        blocker = "Pick a model to continue.";
    } else if (active?.status !== "ready") {
        const downloading =
            models.pendingUse !== null ||
            (active !== undefined &&
                (active.status === "downloading" || models.progress[active.id] !== undefined));
        blocker = downloading
            ? "Downloading — you can continue once it finishes."
            : "Download the model to continue.";
    } else if (isAppleSpeech && speech !== "granted") {
        blocker = "Apple Speech needs Speech Recognition access.";
    } else {
        ready = true;
    }

    return (
        <>
            <div className="sv-onb__icon">
                <AudioLines size={20} />
            </div>
            <h1 className="sv-onb__title">Set up your voice model</h1>
            <p className="sv-onb__lead">
                Parakeet turns your speech into text right on this Mac: fast, private and offline
                once downloaded. Prefer Groq in the cloud, or dictate in Hindi or Hinglish? Choose a
                different model. You can change this any time in Settings.
            </p>
            <div className="sv-onb__wide">
                {/* A block wrapper keeps the switch at its natural width in the flex column. */}
                <div>
                    <VoiceModelModeSwitch mode={smart.mode} onChange={smart.setMode} />
                </div>
                {isSmart ? (
                    <SmartSelectPanel
                        models={models}
                        plan={smart.plan}
                        planError={smart.planError}
                        framed
                    />
                ) : (
                    <>
                        {featured && <ModelDownloadCard model={featured} state={models} />}
                        {featured && (
                            <div className="sv-onb__dl-more">
                                <Button
                                    variant="ghost"
                                    size="sm"
                                    aria-expanded={listOpen}
                                    icon={
                                        listOpen ? (
                                            <ChevronUp size={14} />
                                        ) : (
                                            <ChevronDown size={14} />
                                        )
                                    }
                                    onClick={() => {
                                        setListOpen((open) => !open);
                                    }}
                                >
                                    {listOpen ? "Hide other models" : "Choose a different model"}
                                </Button>
                            </div>
                        )}
                        {showList && <VoiceModelList state={models} />}
                    </>
                )}
                {(isSmart || isGroq) && <GroqKeyField />}
                {isAppleSpeech && speech !== undefined && speech !== "granted" && (
                    <div className="sv-onb__perm-row">
                        <span>Speech Recognition</span>
                        <StatusBadge status={speech} />
                        <PermissionAction
                            kind="speech"
                            status={speech}
                            busy={busy === "speech"}
                            onRequest={request}
                            onOpenSettings={openSettings}
                        />
                    </div>
                )}
            </div>
            <StepFooter
                onBack={onBack}
                next={
                    <div className="sv-onb__next-with-hint">
                        {!ready && <span className="sv-onb__hint">{blocker}</span>}
                        <Button variant="primary" disabled={!ready} onClick={onNext}>
                            Continue
                        </Button>
                    </div>
                }
            />
        </>
    );
}
