// FilePath: src/lib/categories.tsx
// Display metadata for `AppCategory`, shared by the history rows and the insights usage card.
import type { ReactNode } from "react";
import { AppWindow, Briefcase, Code, FileText, Mail, MessageCircle, Sparkles } from "lucide-react";
import type { AppCategory } from "./api";

export const CATEGORY_META: Record<AppCategory, { label: string; icon: ReactNode }> = {
    work_messages: { label: "Work messages", icon: <Briefcase aria-hidden="true" /> },
    personal_messages: { label: "Personal messages", icon: <MessageCircle aria-hidden="true" /> },
    email: { label: "Email", icon: <Mail aria-hidden="true" /> },
    documents: { label: "Documents", icon: <FileText aria-hidden="true" /> },
    ai_prompts: { label: "AI prompts", icon: <Sparkles aria-hidden="true" /> },
    code: { label: "Code", icon: <Code aria-hidden="true" /> },
    other: { label: "Other apps", icon: <AppWindow aria-hidden="true" /> },
};
