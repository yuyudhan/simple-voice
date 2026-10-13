// FilePath: src/features/meetings/JobProgress.tsx
import { Button, ProgressBar, Spinner } from "../../ui";
import type { MeetingJobState } from "./useMeetingJob";

/** A running job's progress line with its Cancel button; renders nothing when idle. */
export function JobProgress({ job, label }: { job: MeetingJobState; label: string }) {
    if (!job.running) return null;
    const fraction = job.progress?.fraction ?? null;
    return (
        <div className="meeting-job" role="status">
            <div className="meeting-job__line">
                <Spinner size={13} />
                <span className="meeting-job__label">{job.progress?.message ?? label}</span>
                {fraction !== null && (
                    <span className="meeting-job__percent">{Math.round(fraction * 100)}%</span>
                )}
                <Button
                    size="sm"
                    variant="ghost"
                    loading={job.cancelling}
                    onClick={() => {
                        void job.cancel();
                    }}
                >
                    Cancel
                </Button>
            </div>
            {fraction !== null && <ProgressBar value={fraction} />}
        </div>
    );
}
