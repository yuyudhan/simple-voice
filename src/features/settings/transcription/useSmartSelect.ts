// FilePath: src/features/settings/transcription/useSmartSelect.ts
// Which way the voice model is picked (Smart Select or a chosen model) and the Smart Select plan
// for the current languages, refetched whenever the languages, the Groq key or a model's
// availability change, since each of those can change the route.
import { useCallback, useEffect, useState } from "react";
import { api, errorMessage, type SmartSelectPlan } from "../../../lib/api";
import { useSettings } from "../../../app/SettingsContext";
import type { ModelsState } from "../models/useModels";

export const SMART_SELECT = "smart-select";

export type VoiceModelMode = "smart" | "choose";

export interface SmartSelectState {
    mode: VoiceModelMode;
    /** Smart Select is stored at once; Choose model only shows the list until a row is picked. */
    setMode: (mode: VoiceModelMode) => void;
    plan: SmartSelectPlan | null;
    planError: string | null;
}

export function useSmartSelect(models: ModelsState): SmartSelectState {
    const { settings, update } = useSettings();
    const { cancelPendingUse } = models;
    const [choosing, setChoosing] = useState(false);
    const [plan, setPlan] = useState<SmartSelectPlan | null>(null);
    const [planError, setPlanError] = useState<string | null>(null);

    const languages = settings.languages.join(",");
    const statuses = (models.models ?? []).map((m) => `${m.id}:${m.status}`).join(",");
    const keyPresent = settings.groqApiKeyPresent;

    useEffect(() => {
        let current = true;
        api.smartSelectPlan().then(
            (next) => {
                if (!current) return;
                setPlan(next);
                setPlanError(null);
            },
            (error: unknown) => {
                if (current) setPlanError(errorMessage(error));
            },
        );
        return () => {
            current = false;
        };
    }, [languages, statuses, keyPresent]);

    const setMode = useCallback(
        (mode: VoiceModelMode) => {
            setChoosing(mode === "choose");
            if (mode === "smart") {
                cancelPendingUse();
                void update({ transcriptionModel: SMART_SELECT });
            }
        },
        [cancelPendingUse, update],
    );

    const mode = settings.transcriptionModel === SMART_SELECT && !choosing ? "smart" : "choose";
    return { mode, setMode, plan, planError };
}
