// FilePath: src/features/onboarding/ModelStep.tsx
// Voice model choice. Continuing requires a usable model: Groq with a verified key, or a local
// model that is downloaded (plus Speech Recognition access for Apple Speech).
import { AudioLines } from "lucide-react";
import { useSettings } from "../../app/SettingsContext";
import { Button } from "../../ui";
import { GroqKeyField } from "../settings/models/GroqKeyField";
import { useModels } from "../settings/models/useModels";
import { VoiceModelList } from "../settings/models/VoiceModelList";
import { PermissionAction, StatusBadge } from "../settings/permissions/PermissionsSection";
import { usePermissions } from "../settings/permissions/usePermissions";
import { StepFooter } from "./StepFooter";

interface Props {
    onBack: () => void;
    onNext: () => void;
}

export function ModelStep({ onBack, onNext }: Props) {
    const { settings } = useSettings();
    const models = useModels();
    const { permissions, busy, request, openSettings } = usePermissions(1500);
    const active = models.models?.find((m) => m.id === settings.transcriptionModel);
    const isGroq = settings.transcriptionModel === "groq-whisper";
    const isAppleSpeech = active?.provider === "apple";
    const speech = permissions?.speech;

    let ready = false;
    let blocker = "";
    if (isGroq) {
        ready = settings.groqApiKeyPresent;
        blocker = "Save a Groq API key to continue, or pick a local model.";
    } else if (active?.status !== "ready") {
        blocker =
            models.pendingUse !== null
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
                <AudioLines size={22} />
            </div>
            <h1 className="sv-onb__title">Choose a voice model</h1>
            <p className="sv-onb__lead">
                Groq Whisper runs in the cloud and needs a free API key. Parakeet and Apple Speech
                run entirely on this Mac once downloaded. You can change this any time in Settings.
            </p>
            <div className="sv-onb__wide">
                <VoiceModelList state={models} />
                {isGroq && <GroqKeyField />}
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
