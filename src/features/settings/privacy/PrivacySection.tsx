// FilePath: src/features/settings/privacy/PrivacySection.tsx
// Settings → Privacy & data: what macOS lets Simple Voice do, where its data lives and how to
// delete it, mirroring where macOS itself puts permissions.
import { DataGroups } from "../data/DataGroups";
import { PermissionsGroup } from "../permissions/PermissionsGroup";

export function PrivacySection() {
    return (
        <>
            <PermissionsGroup />
            <DataGroups />
        </>
    );
}
