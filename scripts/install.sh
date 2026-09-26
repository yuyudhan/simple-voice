#!/usr/bin/env bash
# FilePath: scripts/install.sh
# Installs or updates Simple Voice. Published with every GitHub release as
# releases/latest/download/install.sh; README.md#install has the one-line curl command that
# pipes it into bash.
#
# Piping is safe: all work happens in main, called on the last line, so bash has read the whole
# script before anything runs. Options go after `bash -s --`.
#
# With Homebrew present it installs or upgrades the cask. It installs straight from the GitHub
# release when Homebrew is missing, when Homebrew fails (endpoint-security agents can kill
# Homebrew's sandboxed extraction), or when the app in /Applications was not installed by
# Homebrew. The direct install checks the zip against the release's .sha256 and the bundle's code
# signature before it replaces /Applications/Simple Voice.app.
#
# Options:
#   --no-brew          Install from the GitHub release even when Homebrew is present.
#   --version X.Y.Z    Install that release instead of the latest (implies --no-brew).
set -euo pipefail

repo="yuyudhan/simple-voice"
cask="yuyudhan/tap/simple-voice"
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

usage: bash -s -- [--no-brew] [--version X.Y.Z]   (piped), or bash install.sh [options]

  --no-brew          Install from the GitHub release even when Homebrew is present.
  --version X.Y.Z    Install that release instead of the latest (implies --no-brew).
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

find_brew() {
    if command -v brew >/dev/null 2>&1; then
        command -v brew
    elif [ -x /opt/homebrew/bin/brew ]; then
        echo /opt/homebrew/bin/brew
    fi
}

# Fails when Homebrew cannot do the job, so the caller falls back to a direct install.
install_with_brew() {
    brew=$1
    if "$brew" list --cask simple-voice >/dev/null 2>&1; then
        say "upgrading with Homebrew"
        "$brew" upgrade --cask "$cask" && return 0
    elif [ -e "$app" ]; then
        say "${app} was not installed by Homebrew; updating it directly"
        return 1
    else
        say "installing with Homebrew"
        "$brew" install --cask "$cask" && return 0
    fi
    say "Homebrew could not install Simple Voice; installing from GitHub instead"
    return 1
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

install_direct() {
    version=$1
    case "$version" in
        "" | *[!0-9.]*) fail "not a release version: '${version}'" ;;
    esac
    if [ "$(installed_version)" = "$version" ]; then
        say "Simple Voice ${version} is already installed"
        return 0
    fi

    tmp=$(mktemp -d "${TMPDIR:-/tmp}/simple-voice.XXXXXX")
    trap 'rm -rf "$tmp"' EXIT
    trap 'exit 130' INT TERM

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
    # Ad-hoc signed builds are not notarized; clearing the flag is the same as approving the app
    # once in System Settings, and matches what the Homebrew cask does.
    $sudo xattr -dr com.apple.quarantine "$app" 2>/dev/null || true

    say "installed Simple Voice ${version} in /Applications"
}

main() {
    use_brew=true
    version=""
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --no-brew) use_brew=false ;;
            --version)
                [ "$#" -ge 2 ] || fail "--version needs a value such as 0.0.3"
                version=${2#v}
                use_brew=false
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

    # Homebrew's cask quits the app during an upgrade and install_direct quits it too; either
    # way, reopen it afterwards if it was running.
    was_running=false
    if is_running; then
        was_running=true
    fi

    brew=$(find_brew || true)
    if ! $use_brew || [ -z "$brew" ] || ! install_with_brew "$brew"; then
        if [ -z "$version" ]; then
            version=$(latest_version)
        fi
        install_direct "$version"
    fi

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
