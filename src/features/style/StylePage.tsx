// FilePath: src/features/style/StylePage.tsx
import { Check, ListChecks, Sparkles, Keyboard, Eraser, ArrowRight } from "lucide-react";
import type { Settings, Style } from "../../lib/api";
import { useSettings } from "../../app/SettingsContext";
import { useShell } from "../../app/ShellContext";
import { Badge, Button, Card, Hero } from "../../ui";
import "./style.css";

interface StyleOption {
    value: Style;
    title: string;
    caption: string;
    sample: string;
}

const OPTIONS: StyleOption[] = [
    {
        value: "formal",
        title: "Formal.",
        caption: "Caps + Punctuation",
        sample: "Hey, are you free for lunch tomorrow? Let's do 12 if that works for you.",
    },
    {
        value: "casual",
        title: "Casual",
        caption: "Caps + Less punctuation",
        sample: "Hey are you free for lunch tomorrow? Let's do 12 if that works for you",
    },
];

const HERO_SUBTITLE =
    "Emails, messages, documents, prompts: whatever app you are in, Simple Voice writes in " +
    "the style you pick here.";

const FORMATTING_POINTS = [
    { icon: <Eraser />, text: "Fillers, false starts and self-corrections are removed." },
    { icon: <ListChecks />, text: "Spoken lists become bullets, and steps become numbered lists." },
    { icon: <Keyboard />, text: "Shortcuts are written the way you type them, like Ctrl+Shift+M." },
    { icon: <Sparkles />, text: "Questions and instructions are written down, never answered." },
];

function providerSummary(settings: Settings): { name: string; detail: string; warning?: string } {
    switch (settings.postProcessing) {
        case "groq":
            return {
                name: "Groq",
                detail: settings.groqFormattingModel,
                warning: settings.groqApiKeyPresent ? undefined : "Needs an API key",
            };
        case "apple":
            return { name: "Apple Intelligence", detail: "On this Mac" };
        case "custom":
            return {
                name: "Custom endpoint",
                detail: `${settings.customModel} at ${settings.customBaseUrl}`,
            };
        case "off":
            return {
                name: "Off",
                detail: "Only the built-in clean-up runs: your dictionary, capitals and spacing.",
            };
    }
}

export function StylePage() {
    const { settings, update } = useSettings();
    const { openSettings } = useShell();
    const provider = providerSummary(settings);

    return (
        <>
            <Hero
                title={
                    <>
                        One style for <em>everything</em> you write
                    </>
                }
                subtitle={HERO_SUBTITLE}
            />

            <div className="style-options" role="radiogroup" aria-label="Writing style">
                {OPTIONS.map((option) => {
                    const selected = settings.style === option.value;
                    return (
                        <button
                            key={option.value}
                            type="button"
                            role="radio"
                            aria-checked={selected}
                            className={`style-option${selected ? " is-selected" : ""}`}
                            onClick={() => {
                                if (!selected) void update({ style: option.value });
                            }}
                        >
                            <span className="style-option__head">
                                <span>
                                    <span className="style-option__title">{option.title}</span>
                                    <span className="style-option__caption">{option.caption}</span>
                                </span>
                                <span className="style-option__check" aria-hidden="true">
                                    {selected && <Check />}
                                </span>
                            </span>
                            <span className="style-option__chat" aria-hidden="true">
                                <span className="style-option__avatar">
                                    <span />
                                </span>
                                <span className="style-option__bubble">{option.sample}</span>
                            </span>
                        </button>
                    );
                })}
            </div>

            <Card className="style-note">
                <h2 className="style-note__title">What AI formatting does in both styles</h2>
                <ul className="style-note__list">
                    {FORMATTING_POINTS.map((point) => (
                        <li key={point.text}>
                            <span className="style-note__icon" aria-hidden="true">
                                {point.icon}
                            </span>
                            {point.text}
                        </li>
                    ))}
                </ul>
                <div className="style-provider">
                    <div className="style-provider__text">
                        <span className="caps-label">Formatting provider</span>
                        <span className="style-provider__name">
                            {provider.name}
                            {provider.warning && <Badge tone="warning">{provider.warning}</Badge>}
                        </span>
                        <span className="style-provider__detail">{provider.detail}</span>
                    </div>
                    <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => {
                            openSettings("models");
                        }}
                    >
                        Change in Settings
                        <ArrowRight className="style-provider__arrow" aria-hidden="true" />
                    </Button>
                </div>
            </Card>
        </>
    );
}
