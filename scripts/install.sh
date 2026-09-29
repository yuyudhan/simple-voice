#!/usr/bin/env bash
# FilePath: scripts/install.sh
# Installs, updates or uninstalls Simple Voice. Published with every GitHub release as
# releases/latest/download/install.sh; README.md#install has the one-line curl command that
# pipes it into bash.
#
# Piping is safe: all work happens in main, called on the last line, so bash has read the whole
# script before anything runs. Options go after `bash -s --`.
#
# It installs from the GitHub release: it checks the zip against the release's .sha256 and the
# bundle's code signature before it replaces ~/Applications/Simple Voice.app, quitting the app
# first and reopening it afterwards when it was running.
#
# The app goes in the user's own Applications folder, so installing and updating never need an
# administrator password; Spotlight and Launchpad index that folder like /Applications. A copy
# left in /Applications by an older script is removed when the user may delete it, so only one
# Simple Voice remains.
#
# The app's Install button runs this same pipe without a terminal; a failure after the app quit
# reopens the old app.
#
# Uninstalling quits the app, deletes it and what macOS keeps for it under ~/Library, and
# resets its privacy permissions. The user's data in ~/.simplevoice stays unless --purge is
# given, so a later reinstall picks up history, dictionary and settings again.
#
# Options:
#   --version X.Y.Z    Install that release instead of the latest.
#   --uninstall        Remove the app and its permissions; keep the data in ~/.simplevoice.
#   --purge            With --uninstall, also delete ~/.simplevoice and a moved database.
set -euo pipefail

repo="yuyudhan/simple-voice"
bundle_id="dev.yuyudhan.simplevoice"
apps_dir="${HOME}/Applications"
app="${apps_dir}/Simple Voice.app"
legacy_app="/Applications/Simple Voice.app"
# What tauri-plugin-autostart wrote before the app switched to SMAppService.
legacy_agent="${HOME}/Library/LaunchAgents/Simple Voice.plist"
data_dir="${HOME}/.simplevoice"
database_file="simple-voice.db"
# Matches the bundle's executable wherever it is installed, but not the engine helper beside it
# (simple-voice-engine), which exits on its own once the app is gone.
executable_pattern="Simple Voice\.app/Contents/MacOS/simple-voice( |$)"

say() {
    printf 'simple-voice: %s\n' "$*"
}

fail() {
    printf 'simple-voice: %s\n' "$*" >&2
    exit 1
}

usage() {
    cat <<'EOF'
Install, update or uninstall Simple Voice.

usage: bash -s -- [options]   (piped), or bash install.sh [options]

  --version X.Y.Z    Install that release instead of the latest.
  --uninstall        Remove the app and its permissions; keep the data in ~/.simplevoice.
  --purge            With --uninstall, also delete ~/.simplevoice (history, dictionary,
                     settings, models) and the database in a folder it was moved to.
EOF
}

check_macos() {
    [ "$(uname -s)" = "Darwin" ] || fail "Simple Voice runs on macOS only"
}

check_platform() {
    check_macos
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

# -a: BSD pgrep and pkill skip their own ancestors by default, and when the app's Install button
# runs this script the app is one of them, so without it the app is never seen or quit.
is_running() {
    pgrep -a -f "$executable_pattern" >/dev/null 2>&1
}

# The copy to reopen: the new one once it is in place, else the one an older script installed.
current_app() {
    if [ -d "$app" ]; then
        echo "$app"
    elif [ -d "$legacy_app" ]; then
        echo "$legacy_app"
    fi
}

# Deleting an item needs write access to it and to its folder; standard users usually have
# neither for /Applications, so the old copy then stays and the user is told.
remove_legacy_app() {
    [ -e "$legacy_app" ] || return 0
    if [ -w /Applications ] && [ -w "$legacy_app" ] && rm -rf "$legacy_app" 2>/dev/null; then
        say "removed the old copy in /Applications"
    else
        say "an old copy remains in ${legacy_app}; an administrator can delete it"
    fi
}

# A signal, not `osascript -e 'quit app ...'`: an Apple Event needs Automation permission, and
# when the app's own Install button runs this script macOS attributes the event to the app,
# whose hardened runtime lacks the apple-events entitlement, so the quit is silently refused.
# The app keeps nothing unsaved, and its engine helper exits when the app's end of its stdin
# closes, so TERM is a clean quit.
quit_app() {
    say "quitting Simple Voice"
    pkill -TERM -a -f "$executable_pattern" 2>/dev/null || true
    waited=0
    while is_running; do
        waited=$((waited + 1))
        if [ "$waited" -eq 20 ]; then
            pkill -KILL -a -f "$executable_pattern" 2>/dev/null || true
        fi
        [ "$waited" -le 30 ] ||
            fail "Simple Voice did not quit; quit it from the menu bar and install again"
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
    # The bar redraws with carriage returns, which fill the app's update.log with noise.
    if [ -t 2 ]; then
        progress=--progress-bar
    else
        progress=--silent
    fi
    curl -fL --show-error "$progress" -o "${tmp}/${asset}" "${base}/${asset}" ||
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

    if is_running; then
        quit_app
    fi

    mkdir -p "$apps_dir"
    # Stage beside the destination so a failed copy never leaves the folder without the app.
    staged="${apps_dir}/.Simple Voice.app.installing"
    rm -rf "$staged"
    ditto "$new" "$staged"
    rm -rf "$app"
    mv "$staged" "$app"
    # Releases are not notarized; clearing the flag is the same as approving the app once in
    # System Settings.
    xattr -dr com.apple.quarantine "$app" 2>/dev/null || true

    say "installed Simple Voice ${version} in ${apps_dir}"
    remove_legacy_app
}

cleanup() {
    code=$?
    rm -rf "$work"
    # A failure after the app quit must not leave the user without it.
    if [ "$code" -ne 0 ] && $was_running && ! is_running; then
        reopen=$(current_app)
        if [ -n "$reopen" ]; then
            open "$reopen" || true
        fi
    fi
}

# A database moved in Settings > Privacy & data sits in a folder the user chose, which may hold
# their own files, so only the database files go and the folder stays.
remove_moved_database() {
    local pointer="${data_dir}/location" dir suffix
    [ -f "$pointer" ] || return 0
    dir=$(<"$pointer")
    case "$dir" in
        /*) ;;
        *) return 0 ;;
    esac
    if [ ! -d "$dir" ]; then
        say "the moved database folder ${dir} is not available; delete ${database_file} there yourself"
        return 0
    fi
    for suffix in "" "-wal" "-shm"; do
        rm -f "${dir}/${database_file}${suffix}"
    done
    say "deleted the database in ${dir}"
}

uninstall() {
    local purge=$1 id
    if is_running; then
        quit_app
    fi

    if [ -e "$app" ]; then
        rm -rf "$app"
        say "removed ${app}"
    fi
    remove_legacy_app

    # Development builds use ".dev" identifiers and are deliberately left alone.
    for id in "$bundle_id" "${bundle_id}.engine"; do
        rm -rf "${HOME}/Library/Caches/${id}" "${HOME}/Library/HTTPStorages/${id}" \
            "${HOME}/Library/WebKit/${id}" "${HOME}/Library/Saved Application State/${id}.savedState"
        # Through cfprefsd, which would otherwise write a cached copy back.
        defaults delete "$id" >/dev/null 2>&1 || true
        # Fails when macOS holds no grant for the identifier, which is fine.
        tccutil reset All "$id" >/dev/null 2>&1 || true
    done
    rm -f "$legacy_agent"
    say "reset the Microphone, Accessibility and Speech Recognition permissions"

    if $purge; then
        remove_moved_database
        rm -rf "$data_dir"
        say "deleted ${data_dir}"
    elif [ -d "$data_dir" ]; then
        say "kept your data in ${data_dir}; rerun with --uninstall --purge to delete it"
    fi
    say "Simple Voice is uninstalled"
}

main() {
    version=""
    action="install"
    purge=false
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --version)
                [ "$#" -ge 2 ] || fail "--version needs a value such as 0.0.3"
                version=${2#v}
                shift
                ;;
            --uninstall) action="uninstall" ;;
            --purge) purge=true ;;
            -h | --help)
                usage
                return 0
                ;;
            *) fail "unknown option '$1' (see --help)" ;;
        esac
        shift
    done

    if [ "$action" = "uninstall" ]; then
        [ -z "$version" ] || fail "--version and --uninstall cannot be combined"
        check_macos
        uninstall "$purge"
        return 0
    fi
    if $purge; then
        fail "--purge only goes with --uninstall"
    fi

    check_platform

    # install_release quits the app; reopen it afterwards if it was running.
    was_running=false
    if is_running; then
        was_running=true
    fi

    work=$(mktemp -d "${TMPDIR:-/tmp}/simple-voice.XXXXXX")
    trap cleanup EXIT
    trap 'exit 130' INT TERM

    if [ -z "$version" ]; then
        version=$(latest_version)
    fi
    install_release "$version"

    if $was_running && ! is_running; then
        open "$app"
    elif ! is_running; then
        say "open Simple Voice from Spotlight or ${apps_dir}"
    fi

    cat <<'EOF'

macOS ties the Microphone, Accessibility and Speech Recognition permissions to the app's
signature. If dictation stops recording or pasting after an update, open System Settings >
Privacy & Security, remove Simple Voice from the affected list, and grant it again from the
app's Settings > Privacy & data.
EOF
}

main "$@"
