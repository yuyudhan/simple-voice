// FilePath: src/features/onboarding/StepFooter.tsx
import { useState, type ReactNode } from "react";
import { ArrowLeft, TriangleAlert } from "lucide-react";
import { Button } from "../../ui";

interface Skip {
    label: string;
    /** Shown before skipping so the consequence is explicit. */
    warning: string;
    onSkip: () => void;
}

interface Props {
    onBack?: () => void;
    next: ReactNode;
    skip?: Skip;
}

export function StepFooter({ onBack, next, skip }: Props) {
    const [confirmSkip, setConfirmSkip] = useState(false);

    return (
        <div className="sv-onb__footer-wrap">
            {skip && confirmSkip && (
                <div className="sv-onb__skip-warning" role="alert">
                    <TriangleAlert size={16} />
                    <span>{skip.warning}</span>
                    <Button size="sm" variant="secondary" onClick={skip.onSkip}>
                        Skip anyway
                    </Button>
                </div>
            )}
            <div className="sv-onb__footer">
                {onBack ? (
                    <Button variant="ghost" icon={<ArrowLeft size={15} />} onClick={onBack}>
                        Back
                    </Button>
                ) : (
                    <span />
                )}
                <div className="sv-onb__footer-right">
                    {skip && !confirmSkip && (
                        <Button
                            variant="ghost"
                            onClick={() => {
                                setConfirmSkip(true);
                            }}
                        >
                            {skip.label}
                        </Button>
                    )}
                    {next}
                </div>
            </div>
        </div>
    );
}
