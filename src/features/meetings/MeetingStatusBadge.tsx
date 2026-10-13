// FilePath: src/features/meetings/MeetingStatusBadge.tsx
import type { MeetingStatus } from "../../lib/api";
import { Badge } from "../../ui";

export function MeetingStatusBadge({ status }: { status: MeetingStatus }) {
    switch (status) {
        case "recording":
            return <Badge tone="accent">Recording</Badge>;
        case "processing":
            return <Badge tone="accent">Processing</Badge>;
        case "ready":
            return <Badge tone="success">Ready</Badge>;
        case "failed":
            return <Badge tone="danger">Failed</Badge>;
        case "cancelled":
            return <Badge tone="warning">Cancelled</Badge>;
    }
}
