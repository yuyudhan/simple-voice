#!/usr/bin/env bash
# FilePath: scripts/install.sh
# Installs or updates Simple Voice. Published with every GitHub release as
# releases/latest/download/install.sh; README.md#install has the one-line curl command that
# pipes it into bash.
#
# Piping is safe: all work happens in main, called on the last line, so bash has read the whole
# script before anything runs. Options go after `bash -s --`.
#
# It installs from the GitHub release: it checks the zip against the release's .sha256 and the
# bundle's code signature before it replaces /Applications/Simple Voice.app, quitting the app
# first and reopening it afterwards when it was running.
#
# The app's Install button runs this same pipe without a terminal; sudo then asks for the
# administrator password in a dialog, and a failure after the app quit reopens the old app.
#
# Options:
#   --version X.Y.Z    Install that release instead of the latest.
set -euo pipefail

repo="yuyudhan/simple-voice"
bundle_id="dev.yuyudhan.simplevoice"
app="/Applications/Simple Voice.app"
executable="${app}/Contents/MacOS/simple-voice"

say() {
    printf 'simple-voice: %s\n' "$*"
}

fail() {
    printf 'simple-voice: %s\n' "$*" >&2
    exit 1
}

usage() {
    cat <<'EOF'
Install or update Simple Voice.

usage: bash -s -- [--version X.Y.Z]   (piped), or bash install.sh [options]

  --version X.Y.Z    Install that release instead of the latest.
EOF
}

check_platform() {
    [ "$(uname -s)" = "Darwin" ] || fail "Simple Voice runs on macOS only"
    # `uname -m` reports x86_64 under Rosetta, so ask the hardware instead.
    [ "$(sysctl -n hw.optional.arm64 2>/dev/null || echo 0)" = "1" ] ||
        fail "Simple Voice needs an Apple Silicon Mac"
    major=$(sw_vers -productVersion | cut -d. -f1)
    [ "$major" -ge 14 ] || fail "Simple Voice needs macOS 14 (Sonoma) or later"
}

latest_version() {
    # github.com/<repo>/releases/latest redirects to the newest tag; no API token or JSON needed.
    url=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/${repo}/releases/latest") ||
        fail "could not reach GitHub to find the latest release"
    tag=${url##*/}
    echo "${tag#v}"
}

installed_version() {
    /usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' \
        "${app}/Contents/Info.plist" 2>/dev/null || true
}

is_running() {
    pgrep -f "$executable" >/dev/null 2>&1
}

quit_app() {
    say "quitting Simple Voice"
    osascript -e "quit app id \"${bundle_id}\"" >/dev/null 2>&1 || true
    waited=0
    while is_running; do
        waited=$((waited + 1))
        [ "$waited" -le 20 ] || fail "Simple Voice did not quit; quit it from the menu bar and rerun"
        sleep 0.5
    done
}

install_release() {
    version=$1
    case "$version" in
        "" | *[!0-9.]*) fail "not a release version: '${version}'" ;;
    esac
    if [ "$(installed_version)" = "$version" ]; then
        say "Simple Voice ${version} is already installed"
        return 0
    fi

    tmp="${work}/download"
    mkdir "$tmp"

    asset="Simple-Voice_${version}_aarch64.zip"
    base="https://github.com/${repo}/releases/download/v${version}"
    say "downloading Simple Voice ${version}"
    curl -fL --progress-bar -o "${tmp}/${asset}" "${base}/${asset}" ||
        fail "download failed: ${base}/${asset}"
    curl -fsSL -o "${tmp}/${asset}.sha256" "${base}/${asset}.sha256" ||
        fail "download failed: ${base}/${asset}.sha256"
    (cd "$tmp" && shasum -a 256 -c "${asset}.sha256" >/dev/null) ||
        fail "checksum mismatch for ${asset}; nothing was installed"

    ditto -x -k "${tmp}/${asset}" "${tmp}/unpacked"
    new="${tmp}/unpacked/Simple Voice.app"
    [ -d "$new" ] || fail "${asset} does not contain Simple Voice.app"
    codesign --verify --deep --strict "$new" 2>/dev/null ||
        fail "the downloaded app's code signature is invalid; nothing was installed"

    sudo=""
    if [ ! -w /Applications ] || { [ -e "$app" ] && [ ! -w "$app" ]; }; then
        say "writing to /Applications needs an administrator password"
        sudo="sudo"
        if [ -n "${SUDO_ASKPASS:-}" ]; then
            sudo="sudo -A"
        fi
        # Ask before quitting the app, so a cancelled prompt leaves it running.
        $sudo -v || fail "no administrator password; nothing was installed"
    fi

    if is_running; then
        quit_app
    fi

    # Stage beside the destination so a failed copy never leaves /Applications without the app.
    staged="/Applications/.Simple Voice.app.installing"
    $sudo rm -rf "$staged"
    $sudo ditto "$new" "$staged"
    $sudo rm -rf "$app"
    $sudo mv "$staged" "$app"
    # Releases are not notarized; clearing the flag is the same as approving the app once in
    # System Settings.
    $sudo xattr -dr com.apple.quarantine "$app" 2>/dev/null || true

    say "installed Simple Voice ${version} in /Applications"
}

write_askpass() {
    cat >"$1" <<'EOF'
#!/bin/sh
exec /usr/bin/osascript \
    -e 'text returned of (display dialog "Simple Voice needs an administrator password to install the update." default answer "" with hidden answer with title "Simple Voice" with icon caution)'
EOF
    chmod 700 "$1"
}

cleanup() {
    code=$?
    rm -rf "$work"
    # A failure after the app quit must not leave the user without it.
    if [ "$code" -ne 0 ] && $was_running && ! is_running && [ -d "$app" ]; then
        open "$app" || true
    fi
}

main() {
    version=""
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --version)
                [ "$#" -ge 2 ] || fail "--version needs a value such as 0.0.3"
                version=${2#v}
                shift
                ;;
            -h | --help)
                usage
                return 0
                ;;
            *) fail "unknown option '$1' (see --help)" ;;
        esac
        shift
    done

    check_platform

    # install_release quits the app; reopen it afterwards if it was running.
    was_running=false
    if is_running; then
        was_running=true
    fi

    work=$(mktemp -d "${TMPDIR:-/tmp}/simple-voice.XXXXXX")
    trap cleanup EXIT
    trap 'exit 130' INT TERM

    # No terminal means the app started this; sudo then asks in a dialog.
    if ! (: </dev/tty) 2>/dev/null; then
        write_askpass "${work}/askpass"
        export SUDO_ASKPASS="${work}/askpass"
    fi

    if [ -z "$version" ]; then
        version=$(latest_version)
    fi
    install_release "$version"

    if $was_running && ! is_running; then
        open "$app"
    elif ! is_running; then
        say "open Simple Voice from Applications"
    fi

    cat <<'EOF'

macOS ties the Microphone, Accessibility and Speech Recognition permissions to the app's
signature. If dictation stops recording or pasting after an update, open System Settings >
Privacy & Security, remove Simple Voice from the affected list, and grant it again from the
app's Settings > Permissions.
EOF
}

main "$@"
