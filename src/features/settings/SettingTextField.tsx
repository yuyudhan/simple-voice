// FilePath: src/features/settings/SettingTextField.tsx
// Text setting that saves on blur or Enter rather than on every keystroke, so half-typed URLs and
// model names never reach the backend validator.
import { useState, type ComponentProps } from "react";
import { TextField } from "../../ui";

interface Props extends Omit<ComponentProps<typeof TextField>, "value" | "onChange" | "onBlur"> {
    value: string;
    onCommit: (value: string) => Promise<unknown>;
}

function DraftField({ value, onCommit, ...rest }: Props) {
    const [draft, setDraft] = useState(value);
    const commit = () => {
        const next = draft.trim();
        if (next === value) return;
        void onCommit(next);
    };
    return (
        <TextField
            {...rest}
            value={draft}
            onChange={(event) => {
                setDraft(event.target.value);
            }}
            onBlur={commit}
            onKeyDown={(event) => {
                if (event.key === "Enter") commit();
            }}
        />
    );
}

/** Remounts when the stored value changes elsewhere so the draft never shows stale text. */
export function SettingTextField(props: Props) {
    return <DraftField key={props.value} {...props} />;
}
