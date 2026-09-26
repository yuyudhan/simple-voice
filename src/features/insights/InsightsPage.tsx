// FilePath: src/features/insights/InsightsPage.tsx
import { useState } from "react";
import { PageHeader, Tabs } from "../../ui";
import { ModelInsightsPanel } from "./ModelInsightsPanel";
import { PersonalInsights } from "./PersonalInsights";
import "./insights.css";

type InsightsTab = "personal" | "models";

const TABS: { id: InsightsTab; label: string }[] = [
    { id: "personal", label: "Personal" },
    { id: "models", label: "Models" },
];

function isInsightsTab(id: string): id is InsightsTab {
    return TABS.some((tab) => tab.id === id);
}

export function InsightsPage() {
    const [tab, setTab] = useState<InsightsTab>("personal");

    return (
        <>
            <PageHeader
                title="Insights"
                description="How you dictate and how fast each model works, measured on this Mac."
            />
            <div className="insights-tabs">
                <Tabs
                    items={TABS}
                    value={tab}
                    onChange={(id) => {
                        if (isInsightsTab(id)) setTab(id);
                    }}
                />
            </div>
            <div role="tabpanel" aria-label={tab === "personal" ? "Personal" : "Models"}>
                {tab === "personal" ? <PersonalInsights /> : <ModelInsightsPanel />}
            </div>
        </>
    );
}
