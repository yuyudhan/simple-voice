// FilePath: src/features/settings/models/FormattingProviders.tsx
// Post-processing provider choice plus the fields each provider needs and a live test that
// formats a fixed sample through the configured provider.
import { useState, type SyntheticEvent, type ReactNode } from "react";
import { Apple, CircleSlash, Eye, EyeOff, Server, Zap } from "lucide-react";
import {
    api,
    errorMessage,
    type ModelInfo,
    type PostProcessing,
    type PostProcessingTest,
} from "../../../lib/api";
import { useSettings } from "../../../app/SettingsContext";
import { Button, IconButton, TextField } from "../../../ui";
import { SettingTextField } from "../SettingTextField";
import "../common.css";
import "./models.css";

interface ProviderOption {
    id: PostProcessing;
    name: string;
    description: string;
    icon: ReactNode;
}

const PROVIDERS: ProviderOption[] = [
    {
        id: "groq",
        name: "Groq",
        description: "Remote, fastest. The default.",
        icon: <Zap size={16} />,
    },
    {
        id: "apple",
        name: "Apple Intelligence",
        description: "On-device, macOS 26+.",
        icon: <Apple size={16} />,
    },
    {
        id: "custom",
        name: "Custom",
        description: "OpenAI-compatible: Ollama, LM Studio or any remote.",
        icon: <Server size={16} />,
    },
    {
        id: "off",
        name: "Off",
        description: "Rules and capitalisation only.",
        icon: <CircleSlash size={16} />,
    },
];

function CustomKeyField() {
    const { settings, refresh } = useSettings();
    const [key, setKey] = useState("");
    const [reveal, setReveal] = useState(false);
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const apply = async (value: string | null) => {
        setBusy(true);
        setError(null);
        try {
            await api.setCustomApiKey(value);
            await refresh();
            setKey("");
        } catch (e) {
            setError(errorMessage(e));
        } finally {
            setBusy(false);
        }
    };

    const submit = (event: SyntheticEvent) => {
        event.preventDefault();
        const trimmed = key.trim();
        if (trimmed !== "") void apply(trimmed);
    };

    return (
        <div className="sv-formatting__keyblock">
            <span className="sv-formatting__label">API key (optional)</span>
            <form className="sv-keycard__form" onSubmit={submit}>
                <TextField
                    aria-label="Custom endpoint API key"
                    error={error ?? undefined}
                    type={reveal ? "text" : "password"}
                    value={key}
                    placeholder={settings.customApiKeyPresent ? "Saved — enter to replace" : ""}
                    autoComplete="off"
                    spellCheck={false}
                    onChange={(event) => {
                        setKey(event.target.value);
                    }}
                />
                <IconButton
                    type="button"
                    label={reveal ? "Hide key" : "Show key"}
                    icon={reveal ? <EyeOff size={15} /> : <Eye size={15} />}
                    onClick={() => {
                        setReveal((r) => !r);
                    }}
                />
                <Button type="submit" variant="secondary" loading={busy && key !== ""}>
                    Save
                </Button>
                {settings.customApiKeyPresent && (
                    <Button
                        type="button"
                        variant="ghost"
                        loading={busy && key === ""}
                        onClick={() => {
                            void apply(null);
                        }}
                    >
                        Remove
                    </Button>
                )}
            </form>
            <p className="sv-inline-note">
                {settings.customApiKeyPresent
                    ? "A key is saved for this endpoint."
                    : "Local servers such as Ollama and LM Studio need no key."}
            </p>
        </div>
    );
}

function TestFormatting() {
    const [running, setRunning] = useState(false);
    const [result, setResult] = useState<PostProcessingTest | null>(null);
    const [error, setError] = useState<string | null>(null);

    const run = async () => {
        setRunning(true);
        setError(null);
        setResult(null);
        try {
            setResult(await api.testPostProcessing());
        } catch (e) {
            setError(errorMessage(e));
        } finally {
            setRunning(false);
        }
    };

    return (
        <div className="sv-formatting__test">
            <div>
                <Button
                    variant="secondary"
                    size="sm"
                    loading={running}
                    onClick={() => {
                        void run();
                    }}
                >
                    Test formatting
                </Button>
            </div>
            {result && (
                <>
                    <pre className="sv-formatting__output">{result.output}</pre>
                    <span className="sv-formatting__latency">
                        Formatted in {result.latencyMs} ms
                    </span>
                </>
            )}
            {error && (
                <p className="sv-inline-error" role="alert">
                    {error}
                </p>
            )}
        </div>
    );
}

export function FormattingProviders({ appleIntelligence }: { appleIntelligence?: ModelInfo }) {
    const { settings, update } = useSettings();
    const appleUnavailable = appleIntelligence?.status === "unsupported";

    return (
        <div className="sv-formatting__detail">
            <p className="sv-formatting__note">
                Any voice model works with any formatting provider. Pair Parakeet or Apple Speech
                with Apple Intelligence (or a local Ollama) to stay fully offline.
            </p>
            <div className="sv-providers" role="radiogroup" aria-label="Formatting provider">
                {PROVIDERS.map((provider) => {
                    const disabled =
                        provider.id === "apple" &&
                        appleUnavailable &&
                        settings.postProcessing !== "apple";
                    return (
                        <button
                            key={provider.id}
                            type="button"
                            role="radio"
                            aria-checked={settings.postProcessing === provider.id}
                            className="sv-provider-card"
                            disabled={disabled}
                            title={
                                provider.id === "apple" && appleUnavailable
                                    ? (appleIntelligence.reason ?? undefined)
                                    : undefined
                            }
                            onClick={() => {
                                void update({ postProcessing: provider.id });
                            }}
                        >
                            <span className="sv-provider-card__icon">{provider.icon}</span>
                            <span className="sv-provider-card__name">{provider.name}</span>
                            <span className="sv-provider-card__desc">{provider.description}</span>
                        </button>
                    );
                })}
            </div>

            {settings.postProcessing === "groq" && (
                <SettingTextField
                    label="Groq formatting model"
                    hint="Default qwen/qwen3.8-27b. Any Groq chat model id works."
                    value={settings.groqFormattingModel}
                    spellCheck={false}
                    onCommit={(groqFormattingModel) => update({ groqFormattingModel })}
                />
            )}

            {settings.postProcessing === "apple" && appleIntelligence && (
                <p className={appleUnavailable ? "sv-inline-warning" : "sv-inline-note"}>
                    {appleUnavailable
                        ? (appleIntelligence.reason ?? "Apple Intelligence is not available.")
                        : "Apple Intelligence is available and runs entirely on this Mac."}
                </p>
            )}

            {settings.postProcessing === "custom" && (
                <>
                    <SettingTextField
                        label="Base URL"
                        hint="The OpenAI-compatible root, e.g. http://localhost:11434/v1 for Ollama."
                        value={settings.customBaseUrl}
                        spellCheck={false}
                        placeholder="http://localhost:11434/v1"
                        onCommit={(customBaseUrl) => update({ customBaseUrl })}
                    />
                    <SettingTextField
                        label="Model"
                        hint="The model name the endpoint expects, e.g. qwen3:8b."
                        value={settings.customModel}
                        spellCheck={false}
                        onCommit={(customModel) => update({ customModel })}
                    />
                    <CustomKeyField />
                </>
            )}

            {settings.postProcessing === "off" ? (
                <p className="sv-inline-note">
                    Only your dictionary rules, brand casing, spacing and capitalisation are
                    applied.
                </p>
            ) : (
                <TestFormatting />
            )}
        </div>
    );
}
