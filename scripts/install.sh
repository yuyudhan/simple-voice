#!/bin/sh
# FilePath: scripts/install.sh
# Installs, updates or uninstalls Simple Voice for the current user, without sudo:
#     curl -fsSL https://yuyudhan.github.io/simple-voice/install | sh [-s -- --uninstall]
# That URL serves this file as attached to the latest GitHub release, so `sh install.sh` and the
# older `curl … | bash` work the same. POSIX sh (sh, dash, bash, zsh); everything is a function
# and the last line calls main, so a download cut short runs nothing. On Linux it hands over to
# install-linux.sh from the same release, checked against its SHA256SUMS; the rest is macOS.
#
# It checks the zip's SHA-256 against the release's .sha256 and the bundle's code signature,
# then replaces ~/Applications/Simple Voice.app (no administrator password; a copy an older
# script left in /Applications is removed when allowed), quitting the app first and reopening it
# after. When that release is installed but the running app is older, the app is only restarted.
# The app's Install button pipes this script to bash without a terminal and shows the last
# `simple-voice:` line as the reason, so every failure is exactly one such line with exit 1; a
# failure after the app quit reopens the old app. Downloads go into one mktemp -d folder removed
# on every exit (success, error, HUP, INT, TERM); nothing is written to the current folder.
# Colour only on a terminal, without NO_COLOR, and TERM not dumb.
#
# Uninstalling quits the app, deletes it and what macOS keeps for it under ~/Library, and resets
# its privacy permissions; ~/.simplevoice stays unless --purge. Options and environment: usage.

# zsh (`… | zsh`) runs it as sh would, so an EXIT trap fires at exit. Before `set` (emulate resets it).
if [ -n "${ZSH_VERSION:-}" ]; then
    emulate sh
fi
set -eu

repo="yuyudhan/simple-voice"
releases_url="https://github.com/${repo}/releases"
install_url="https://yuyudhan.github.io/simple-voice/install"
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

# The private temporary folder of this run; removed on every exit.
work=""
# Whether Simple Voice was running when the install started; set by install_main.
was_running=false
bold="" # these escape sequences are set by setup_colour on a colour terminal
green=""
yellow=""
dim=""
reset=""
red=""
red_reset=""

setup_colour() {
    esc="$(printf '\033')"
    if [ -z "${NO_COLOR:-}" ] && [ -n "${TERM:-}" ] && [ "$TERM" != dumb ]; then
        if [ -t 1 ]; then
            bold="${esc}[1m"
            green="${esc}[32m"
            yellow="${esc}[33m"
            dim="${esc}[2m"
            reset="${esc}[0m"
        fi
        if [ -t 2 ]; then
            red="${esc}[31m"
            red_reset="${esc}[0m"
        fi
    fi
}

# One checked step: `  ✓ TEXT DETAIL`, the detail dimmed.
step() {
    if [ "$#" -gt 1 ]; then
        printf '  %s✓%s %s %s%s%s\n' "$green" "$reset" "$1" "$dim" "$2" "$reset"
    else
        printf '  %s✓%s %s\n' "$green" "$reset" "$1"
    fi
}

# Something the user may want to act on that does not stop the install: `  ! TEXT`.
note() {
    printf '  %s!%s %s\n' "$yellow" "$reset" "$1"
}

# The one error line, newlines in it folded into spaces, and exit 1.
fail() {
    f_message="$(printf '%s' "$*" | tr '\n' ' ')"
    printf 'simple-voice: %serror%s: %s\n' "$red" "$red_reset" "$f_message" >&2
    exit 1
}

# A path with the home folder written as ~.
pretty() {
    case "$1" in
        "$HOME"/*) printf '%s/%s\n' '~' "${1#"$HOME"/}" ;;
        *) printf '%s\n' "$1" ;;
    esac
}

usage() {
    cat <<EOF
Install, update or uninstall Simple Voice (macOS on Apple silicon, Debian/Ubuntu x86-64; no sudo).
Usage: sh install.sh [options], or piped from ${install_url}: ... | sh -s -- [options]
  --version X.Y.Z    install that release instead of the latest (vX.Y.Z works too)
  --uninstall        remove the app and its permissions; keep the data in ~/.simplevoice
  --purge            with --uninstall, also delete ~/.simplevoice (history, dictionary,
                     settings, models) and the database in a folder it was moved to
  -h, --help         show this help
Environment: SIMPLE_VOICE_VERSION (as --version), SIMPLE_VOICE_RELEASES (asset URL base for a
mirror or file:///path holding vX.Y.Z/; needs SIMPLE_VOICE_VERSION or --version).
EOF
}

# curl with retries for transient failures, a limit on a connection that never opens, and an
# abort for a transfer that stalls (under 1 KB/s for 30 s) instead of hanging.
curl_net() {
    curl --connect-timeout 15 --retry 3 --retry-delay 2 --speed-limit 1024 --speed-time 30 "$@" </dev/null
}

# Why the last curl failed, from its stderr in ${work}/curl.err: its own message, no prefix.
curl_error() {
    sed -n 's/^curl: ([0-9]*) //p' "${work}/curl.err" | sed -n '$p'
}

# fetch URL FILE WHAT: download URL to FILE, or fail with one line naming what was missing.
fetch() {
    if f_code="$(curl_net -fsSL -w '%{http_code}' -o "$2" "$1" 2>"${work}/curl.err")"; then
        return 0
    fi
    case "$f_code" in
        404) fail "$3 not found (HTTP 404); the versions are at ${releases_url}" ;;
        "" | 000)
            [ "${1#http}" != "$1" ] || fail "$3 not found at $1 ($(curl_error))"
            fail "could not reach github.com ($(curl_error)); check the network or proxy and run the command again"
            ;;
        *) fail "$3 could not be downloaded (HTTP ${f_code}); run the command again" ;;
    esac
}

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1"; else shasum -a 256 "$1"; fi |
        cut -d ' ' -f 1
}

# verify FILE EXPECTED: fail unless FILE's SHA-256 is EXPECTED.
verify() {
    v_actual="$(sha256_of "$1")"
    if [ -z "$2" ] || [ "$v_actual" != "$2" ]; then
        fail "checksum mismatch for $(basename "$1") (expected ${2:-nothing}, got ${v_actual}); nothing was installed, run the command again"
    fi
}

# The latest release's tag: github.com/<repo>/releases/latest redirects to it (no API token).
latest_tag() {
    if ! l_url="$(curl_net -fsSLI -o /dev/null -w '%{url_effective}' "${releases_url}/latest" 2>"${work}/curl.err")"; then
        fail "could not reach github.com ($(curl_error)); check the network or proxy and run the command again"
    fi
    l_tag="${l_url##*/}"
    case "$l_tag" in
        v[0-9]*) printf '%s\n' "$l_tag" ;;
        *) fail "could not tell the latest release from ${l_url}; run the command again" ;;
    esac
}

# The Linux installer is its own script, published beside this one and listed in the release's
# SHA256SUMS. It gets the same options and environment.
run_linux_installer() {
    work="$(mktemp -d "${TMPDIR:-/tmp}/simple-voice.XXXXXX")"
    trap 'rm -rf "$work"' EXIT
    trap 'exit 130' HUP INT TERM
    if [ -n "${SIMPLE_VOICE_RELEASES:-}" ]; then
        # A mirror holds one release; install-linux.sh reads SIMPLE_VOICE_VERSION too.
        [ -n "${SIMPLE_VOICE_VERSION:-}" ] || fail "SIMPLE_VOICE_RELEASES needs SIMPLE_VOICE_VERSION=X.Y.Z on Linux"
        r_base="${SIMPLE_VOICE_RELEASES}/v${SIMPLE_VOICE_VERSION#v}"
    else
        r_base="${releases_url}/download/$(latest_tag)"
    fi
    fetch "${r_base}/install-linux.sh" "${work}/install-linux.sh" "the Linux installer"
    if curl_net -fsSL -o "${work}/SHA256SUMS" "${r_base}/SHA256SUMS" 2>/dev/null; then
        verify "${work}/install-linux.sh" \
            "$(awk '$2 == "install-linux.sh" || $2 == "*install-linux.sh" { print $1 }' "${work}/SHA256SUMS")"
    fi
    r_status=0
    sh "${work}/install-linux.sh" "$@" </dev/null || r_status=$?
    exit "$r_status"
}

check_macos() {
    [ "$(uname -s)" = "Darwin" ] || fail "Simple Voice runs on macOS and Debian/Ubuntu Linux only"
}

check_platform() {
    check_macos
    # `uname -m` reports x86_64 under Rosetta, so ask the hardware instead.
    [ "$(sysctl -n hw.optional.arm64 2>/dev/null || echo 0)" = "1" ] ||
        fail "Simple Voice needs an Apple Silicon Mac"
    macos_version="$(sw_vers -productVersion)"
    [ "${macos_version%%.*}" -ge 14 ] || fail "Simple Voice needs macOS 14 (Sonoma) or later; this Mac runs ${macos_version}"
}

# The latest release's version. A release published from Linux before its Mac build has no zip:
# then the newest release that has one (the API lists releases newest first).
latest_version() {
    [ -z "${SIMPLE_VOICE_RELEASES:-}" ] || fail "SIMPLE_VOICE_RELEASES needs a version: pass --version X.Y.Z"
    l_version="$(latest_tag)"
    l_version="${l_version#v}"
    if ! curl_net -fsLI -o /dev/null \
        "${releases_url}/download/v${l_version}/Simple-Voice_${l_version}_aarch64.zip" 2>/dev/null; then
        l_version="$(curl_net -fsSL "https://api.github.com/repos/${repo}/releases?per_page=30" 2>/dev/null |
            sed -n 's|.*/releases/download/v\([0-9.]*\)/Simple-Voice_[0-9.]*_aarch64\.zip".*|\1|p' |
            sed -n 1p)" || l_version=""
        [ -n "$l_version" ] || fail "no GitHub release has the macOS build yet; see ${releases_url}"
    fi
    printf '%s\n' "$l_version"
}

installed_version() {
    /usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' \
        "${app}/Contents/Info.plist" 2>/dev/null || true
}

# -a: BSD pgrep/pkill skip their ancestors, and the app is one when its Install button runs this.
is_running() {
    pgrep -a -f "$executable_pattern" >/dev/null 2>&1
}

# The copy to reopen: the new one once it is in place, else the one an older script installed.
current_app() {
    if [ -d "$app" ]; then
        printf '%s\n' "$app"
    elif [ -d "$legacy_app" ]; then
        printf '%s\n' "$legacy_app"
    fi
}

# Standard users usually may not delete from /Applications; the old copy then stays, with a note.
remove_legacy_app() {
    [ -e "$legacy_app" ] || return 0
    if [ -w /Applications ] && [ -w "$legacy_app" ] && rm -rf "$legacy_app" 2>/dev/null; then
        step "Removed the old copy in /Applications"
    else
        note "An old copy remains in ${legacy_app}; an administrator can delete it"
    fi
}

# A signal, not `osascript -e 'quit app ...'`: when the app's own Install button runs this script,
# the Apple Event counts as the app's, whose hardened runtime lacks apple-events, so it is refused.
# The app keeps nothing unsaved and its engine helper exits with it, so TERM is a clean quit.
quit_app() {
    pkill -TERM -a -f "$executable_pattern" 2>/dev/null || true
    q_waited=0
    while is_running; do
        q_waited=$((q_waited + 1))
        if [ "$q_waited" -eq 20 ]; then
            pkill -KILL -a -f "$executable_pattern" 2>/dev/null || true
        fi
        [ "$q_waited" -le 30 ] ||
            fail "Simple Voice did not quit; quit it from the menu bar and run the command again"
        sleep 0.5
    done
}

# An older script replaced the bundle without quitting the app, so the old build still runs. ditto
# keeps modification times, so the executable's inode change time records when it was installed.
running_old_build() {
    o_pid="$(pgrep -a -f "$executable_pattern" 2>/dev/null | head -n 1)" || return 1
    [ -n "$o_pid" ] || return 1
    o_started="$(LC_ALL=C ps -o lstart= -p "$o_pid" 2>/dev/null)" || return 1
    o_started="$(LC_ALL=C date -j -f "%a %b %d %T %Y" "$o_started" +%s 2>/dev/null)" || return 1
    o_installed="$(stat -f %c "${app}/Contents/MacOS/simple-voice" 2>/dev/null)" || return 1
    [ "$o_started" -lt "$o_installed" ]
}

# Right after a quit, LaunchServices may take `open` as "activate", which does nothing; try again
# until the app is really up. Fails (status 1) when it never came up.
reopen() {
    p_tries=0
    while [ "$p_tries" -lt 3 ]; do
        p_tries=$((p_tries + 1))
        open "$1" </dev/null 2>/dev/null || true
        p_waited=0
        while [ "$p_waited" -lt 10 ]; do
            if is_running; then
                return 0
            fi
            p_waited=$((p_waited + 1))
            sleep 0.5
        done
    done
    return 1
}

install_release() {
    i_version="$1"
    i_which="$2"
    case "$i_version" in
        "" | *[!0-9.]*) fail "not a release version: '${i_version}'; versions look like 0.4.0" ;;
    esac
    i_asset="Simple-Voice_${i_version}_aarch64.zip"
    if [ "$(installed_version)" = "$i_version" ]; then
        step "macOS ${macos_version} (Apple silicon):" "${i_asset}, ${i_which}"
        step "Simple Voice ${i_version} is already in $(pretty "$apps_dir")"
        if running_old_build; then
            quit_app
            reopen "$app" || fail "could not restart Simple Voice; open it from Spotlight or $(pretty "$apps_dir")"
            step "Restarted Simple Voice to finish the update"
        fi
        return 0
    fi

    i_base="${SIMPLE_VOICE_RELEASES:-${releases_url}/download}/v${i_version}"
    i_tmp="${work}/download"
    mkdir "$i_tmp"
    # The small checksum file first: a missing release or no network fails here, with one line.
    fetch "${i_base}/${i_asset}.sha256" "${i_tmp}/${i_asset}.sha256" "release v${i_version}"
    step "macOS ${macos_version} (Apple silicon):" "${i_asset}, ${i_which}"
    # A progress bar on a terminal only: in the app's update.log its carriage returns are noise.
    if [ -t 2 ]; then
        curl_net -fL --progress-bar -o "${i_tmp}/${i_asset}" "${i_base}/${i_asset}" ||
            fail "the download of ${i_asset} broke off; run the command again"
    else
        fetch "${i_base}/${i_asset}" "${i_tmp}/${i_asset}" "${i_asset}"
    fi
    verify "${i_tmp}/${i_asset}" "$(awk 'NR == 1 { print $1 }' "${i_tmp}/${i_asset}.sha256")"
    ditto -x -k "${i_tmp}/${i_asset}" "${i_tmp}/unpacked" </dev/null ||
        fail "${i_asset} is damaged; nothing was installed, run the command again"
    i_new="${i_tmp}/unpacked/Simple Voice.app"
    [ -d "$i_new" ] || fail "${i_asset} does not contain Simple Voice.app; nothing was installed"
    codesign --verify --deep --strict "$i_new" </dev/null 2>/dev/null ||
        fail "the downloaded app's code signature is invalid; nothing was installed"
    step "Downloaded and verified" "(SHA-256 from ${i_asset}.sha256, code signature)"

    if is_running; then
        quit_app
        step "Quit the running Simple Voice to update it"
    fi

    mkdir -p "$apps_dir"
    # Stage beside the destination so a failed copy never leaves the folder without the app.
    i_staged="${apps_dir}/.Simple Voice.app.installing"
    rm -rf "$i_staged"
    ditto "$i_new" "$i_staged" </dev/null
    rm -rf "$app"
    mv "$i_staged" "$app"
    # Releases are not notarized; clearing the flag is like approving the app in System Settings.
    xattr -dr com.apple.quarantine "$app" 2>/dev/null || true
    step "Installed $(pretty "$app")"
    remove_legacy_app
}

cleanup() {
    c_code=$?
    [ -z "$work" ] || rm -rf "$work"
    # A failure after the app quit must not leave the user without it.
    if [ "$c_code" -ne 0 ] && $was_running && ! is_running; then
        c_previous="$(current_app)"
        if [ -n "$c_previous" ]; then
            reopen "$c_previous" || true
        fi
    fi
}

# A database moved in Settings > Privacy & data sits in a folder the user chose, which may hold
# their own files, so only the database files go and the folder stays.
remove_moved_database() {
    m_pointer="${data_dir}/location"
    [ -f "$m_pointer" ] || return 0
    m_dir="$(cat "$m_pointer")"
    case "$m_dir" in
        /*) ;;
        *) return 0 ;;
    esac
    if [ ! -d "$m_dir" ]; then
        note "The moved database folder ${m_dir} is not available; delete ${database_file} there yourself"
        return 0
    fi
    for m_suffix in "" "-wal" "-shm"; do
        rm -f "${m_dir}/${database_file}${m_suffix}"
    done
    step "Deleted the database in ${m_dir}"
}

uninstall() {
    u_purge="$1"
    printf '%sSimple Voice uninstaller%s\n' "$bold" "$reset"
    if is_running; then
        quit_app
        step "Quit Simple Voice"
    fi

    if [ -e "$app" ]; then
        rm -rf "$app"
        step "Removed $(pretty "$app")"
    fi
    remove_legacy_app

    # Development builds use ".dev" identifiers and are deliberately left alone.
    for u_id in "$bundle_id" "${bundle_id}.engine"; do
        rm -rf "${HOME}/Library/Caches/${u_id}" "${HOME}/Library/HTTPStorages/${u_id}" \
            "${HOME}/Library/WebKit/${u_id}" "${HOME}/Library/Saved Application State/${u_id}.savedState"
        # Through cfprefsd, which would otherwise write a cached copy back.
        defaults delete "$u_id" </dev/null >/dev/null 2>&1 || true
        # Fails when macOS holds no grant for the identifier, which is fine.
        tccutil reset All "$u_id" </dev/null >/dev/null 2>&1 || true
    done
    rm -f "$legacy_agent"
    step "Reset the Microphone, Accessibility and Speech Recognition permissions"

    if $u_purge; then
        remove_moved_database
        rm -rf "$data_dir"
        step "Deleted $(pretty "$data_dir")"
    elif [ -d "$data_dir" ]; then
        note "Kept your data in $(pretty "$data_dir"); --uninstall --purge deletes it"
    fi
    printf '%sSimple Voice is uninstalled.%s\n' "$bold" "$reset"
}

install_main() {
    n_version="$1"
    printf '%sSimple Voice installer%s\n' "$bold" "$reset"
    check_platform

    # install_release quits the app; reopen it afterwards if it was running.
    if is_running; then
        was_running=true
    fi

    work="$(mktemp -d "${TMPDIR:-/tmp}/simple-voice.XXXXXX")"
    trap cleanup EXIT
    trap 'exit 130' HUP INT TERM

    if [ -z "$n_version" ]; then
        # On its own line, so that a failure there stops the script (set -e).
        n_version="$(latest_version)"
        install_release "$n_version" "latest release"
    else
        install_release "$n_version" "release v${n_version}"
    fi

    if $was_running && ! is_running; then
        if reopen "$app"; then
            step "Reopened Simple Voice"
        else
            note "Could not reopen Simple Voice; open it from Spotlight or $(pretty "$apps_dir")"
        fi
    fi

    printf '%sSimple Voice %s is installed.%s\n\n' "$bold" "$(installed_version)" "$reset"
    printf 'Next:\n'
    if is_running; then
        printf '  Hold fn and speak. If it stops recording or pasting, grant Simple Voice again in System Settings → Privacy & Security\n'
    else
        printf '  Open Simple Voice from Spotlight, allow Microphone and Accessibility when it asks, then hold fn and speak\n'
    fi
}

main() {
    setup_colour
    if [ "$(uname -s)" = "Linux" ]; then
        run_linux_installer "$@"
    fi
    a_version="${SIMPLE_VOICE_VERSION:-}"
    a_action="install"
    a_purge=false
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --version)
                [ "$#" -ge 2 ] || fail "--version needs a value such as 0.4.0"
                a_version="$2"
                shift
                ;;
            --version=*) a_version="${1#--version=}" ;;
            --uninstall) a_action="uninstall" ;;
            --purge) a_purge=true ;;
            -h | --help)
                usage
                return 0
                ;;
            *) fail "unknown option '$1'; see --help" ;;
        esac
        shift
    done
    a_version="${a_version#v}"

    if [ "$a_action" = "uninstall" ]; then
        [ -z "$a_version" ] || fail "--version and --uninstall cannot be combined"
        check_macos
        uninstall "$a_purge"
        return 0
    fi
    if $a_purge; then
        fail "--purge only goes with --uninstall"
    fi
    install_main "$a_version"
}

main "$@"
