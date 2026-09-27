// FilePath: src/features/settings/SettingsPage.tsx
import type { ReactNode } from "react";
import type { SettingsSection } from "../../app/ShellContext";
import { PageHeader } from "../../ui";
import { AppSection } from "./app/AppSection";
import { DictationSection } from "./dictation/DictationSection";
import { FormattingSection } from "./formatting/FormattingSection";
import { PrivacySection } from "./privacy/PrivacySection";
import { SETTINGS_PAGES } from "./settingsPages";
import { TranscriptionSection } from "./transcription/TranscriptionSection";
import "./common.css";

const CONTENT: Record<SettingsSection, () => ReactNode> = {
    dictation: () => <DictationSection />,
    transcription: () => <TranscriptionSection />,
    formatting: () => <FormattingSection />,
    app: () => <AppSection />,
    privacy: () => <PrivacySection />,
};

/** One settings section as a page of the main window, titled from its sidebar entry. */
export function SettingsPage({ section }: { section: SettingsSection }) {
    const page = SETTINGS_PAGES.find((item) => item.id === section);
    return (
        <>
            <PageHeader title={page?.label} description={page?.description} />
            {CONTENT[section]()}
        </>
    );
}
