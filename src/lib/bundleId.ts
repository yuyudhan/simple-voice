// FilePath: src/lib/bundleId.ts
// Mirrors the backend's bundle-ID check for `appStyles` and `contextExcludedApps`: dot-separated
// non-empty parts of letters, digits, hyphens and underscores, at most 255 characters. Anything
// else can never match an app, so the form says so before saving.
const BUNDLE_ID = /^[A-Za-z0-9_-]+(\.[A-Za-z0-9_-]+)*$/;
const MAX_LENGTH = 255;

/** Why a typed bundle ID can't be used; null when it is well formed. */
export function bundleIdError(id: string): string | null {
    if (id.length === 0) return "Enter the app's bundle ID, like com.apple.mail.";
    if (id.length > MAX_LENGTH || !BUNDLE_ID.test(id)) {
        return `“${id}” is not an app bundle ID (like com.apple.mail)`;
    }
    return null;
}
