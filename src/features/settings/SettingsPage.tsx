// FilePath: src/features/settings/SettingsPage.tsx
import type { ReactNode } from "react";
import type { SettingsSection } from "../../app/ShellContext";
import { PageHeader } from "../../ui";
import { DataSection } from "./data/DataSection";
import { GeneralSection } from "./general/GeneralSection";
import { ModelsSection } from "./models/ModelsSection";
import { PermissionsSection } from "./permissions/PermissionsSection";
import { SETTINGS_PAGES } from "./settingsPages";
import { SystemSection } from "./system/SystemSection";
import "./common.css";

const CONTENT: Record<SettingsSection, () => ReactNode> = {
    general: () => <GeneralSection />,
    system: () => <SystemSection />,
    models: () => <ModelsSection />,
    permissions: () => <PermissionsSection />,
    data: () => <DataSection />,
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
