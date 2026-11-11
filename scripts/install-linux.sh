#!/bin/sh
# FilePath: scripts/install-linux.sh
# Installs, updates or uninstalls Simple Voice on Debian/Ubuntu Linux (x86-64) for the current
# user, without sudo. Published with every GitHub release beside install.sh, which hands over to
# it on Linux after checking it against the release's SHA256SUMS, so the one-line
#     curl -fsSL https://yuyudhan.github.io/simple-voice/install | sh
# works on both systems. POSIX sh, like install.sh; `sh install-linux.sh` works on its own too.
#
# It downloads the release's Simple-Voice_VERSION_amd64.tar.gz, checks its SHA-256 against the
# release's .sha256, and unpacks it into ~/.local/opt/simple-voice (the app and its engine helper
# side by side), links ~/.local/bin/simple-voice to it and adds the menu entry and icons under
# ~/.local/share. The system libraries the app links (WebKitGTK, GTK, the tray indicator) and
# the desktop tools it calls (xdotool, xprop, xclip, wl-clipboard, pactl, secret-tool) come from apt;
# this script never runs sudo: it checks for them and prints the one apt command to run when
# something is missing.
#
# It quits the app first and reopens it afterwards when it was running. Everything is a
# function and the last line calls main, so a download cut short runs nothing. Every download
# goes into one private temporary folder (mktemp -d), removed when the script exits for any
# reason; nothing is written to the current folder. Every failure is one `simple-voice: error:`
# line on stderr and exit status 1. Colour only on a terminal, without NO_COLOR, and TERM not dumb.
#
# Uninstalling quits the app and removes those files, the autostart entry, the webview's data and
# the Groq key in the keyring. The data in $XDG_DATA_HOME/simplevoice stays unless --purge is
# given.
#
# Options:
#   --version X.Y.Z    Install that release instead of the latest (vX.Y.Z works too).
#   --uninstall        Remove the app; keep the data.
#   --purge            With --uninstall, also delete the data and a moved database.
#   -h, --help         Show this help.
#
# Environment:
#   SIMPLE_VOICE_VERSION    The release to install, as --version (the option wins).
#   SIMPLE_VOICE_RELEASES   Replaces https://github.com/yuyudhan/simple-voice/releases/download,
#                           for a mirror or a locally built release (a URL or file:///path holding
#                           vVERSION/ with the assets); needs a version.

# zsh runs it as sh would. Before `set`, which emulate resets.
if [ -n "${ZSH_VERSION:-}" ]; then
    emulate sh
fi
set -eu

repo="yuyudhan/simple-voice"
releases_url="https://github.com/${repo}/releases"
install_url="https://yuyudhan.github.io/simple-voice/install"
bundle_id="dev.yuyudhan.simplevoice"
package="simple-voice"
data_home="${XDG_DATA_HOME:-${HOME}/.local/share}"
prefix="${HOME}/.local/opt/simple-voice"
binary="${prefix}/simple-voice"
link="${HOME}/.local/bin/simple-voice"
desktop_file="${data_home}/applications/simple-voice.desktop"
icons="${data_home}/icons/hicolor"
# An install by an older release of this script: the .deb in /usr/bin.
system_binary="/usr/bin/simple-voice"
# The app as launched by the menu entry, the link, the .deb, or by name from PATH.
pattern="^(${binary}|${link}|${system_binary}|simple-voice)( |$)"
data_dir="${data_home}/simplevoice"
autostart="${XDG_CONFIG_HOME:-${HOME}/.config}/autostart/${bundle_id}.desktop"
database_file="simple-voice.db"
# The shared libraries the app links and the tools it runs, as Debian/Ubuntu package names.
required_packages="libwebkit2gtk-4.1-0 libgtk-3-0 libayatana-appindicator3-1"
tool_packages="xdotool x11-utils xclip wl-clipboard pulseaudio-utils libsecret-tools"

# The private temporary folder of this run; removed on every exit.
work=""
# Escape sequences, set by setup_colour when the stream is a colour terminal.
bold=""
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
Install, update or uninstall Simple Voice on Linux, for your user only (no sudo).

Usage: sh install-linux.sh [options]
Piped from ${install_url}, the options go after "sh -s --":
  ... | sh -s -- --version 0.4.0

Options:
  --version X.Y.Z    install that release instead of the latest (vX.Y.Z works too)
  --uninstall        remove the app; keep the data in ~/.local/share/simplevoice
  --purge            with --uninstall, also delete that data (history, dictionary, settings,
                     models) and the database in a folder it was moved to
  -h, --help         show this help

Environment: SIMPLE_VOICE_VERSION (as --version), SIMPLE_VOICE_RELEASES (asset URL base).
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
            case "$1" in
                http*) fail "could not reach github.com ($(curl_error)); check the network or proxy and run the command again" ;;
                *) fail "$3 not found at $1 ($(curl_error))" ;;
            esac
            ;;
        *) fail "$3 could not be downloaded (HTTP ${f_code}); run the command again" ;;
    esac
}

check_platform() {
    [ "$(uname -s)" = "Linux" ] || fail "this installer is for Linux; use install.sh on macOS"
    [ "$(uname -m)" = "x86_64" ] || fail "Simple Voice for Linux needs an x86-64 computer; this one is $(uname -m)"
    # whisper.cpp and ONNX Runtime are built for AVX2 and FMA.
    grep -q -w avx2 /proc/cpuinfo 2>/dev/null ||
        fail "Simple Voice for Linux needs a CPU with AVX2 (Intel Haswell, AMD Zen or later)"
}

is_installed_package() {
    dpkg-query -W -f='${Status}\n' "$1" 2>/dev/null | grep -q '^install ok installed$'
}

# The packages of the space-separated list $1 that are not installed, space-separated. Without
# dpkg (not Debian or Ubuntu) nothing can be told, and the app's own library check below still
# runs.
missing_packages() {
    command -v dpkg-query >/dev/null 2>&1 || return 0
    p_missing=""
    for p_name in $1; do
        is_installed_package "$p_name" || p_missing="${p_missing:+${p_missing} }${p_name}"
    done
    printf '%s\n' "$p_missing"
}

check_dependencies() {
    d_libraries="$(missing_packages "$required_packages")"
    d_tools="$(missing_packages "$tool_packages")"
    if [ -n "$d_libraries" ]; then
        fail "Simple Voice needs system libraries first: sudo apt install ${d_libraries}${d_tools:+ ${d_tools}}; then run the command again"
    fi
    if [ -n "$d_tools" ]; then
        note "Optional tools are missing (pasting, selected text, muting, the Groq key store): sudo apt install ${d_tools}"
    fi
}

# github.com/<repo>/releases/latest redirects to the newest tag; no API token or JSON needed.
latest_version() {
    [ -z "${SIMPLE_VOICE_RELEASES:-}" ] || fail "SIMPLE_VOICE_RELEASES needs a version: pass --version X.Y.Z"
    if ! l_url="$(curl_net -fsSLI -o /dev/null -w '%{url_effective}' "${releases_url}/latest" 2>"${work}/curl.err")"; then
        fail "could not reach github.com ($(curl_error)); check the network or proxy and run the command again"
    fi
    l_tag="${l_url##*/}"
    case "$l_tag" in
        v[0-9]*) printf '%s\n' "${l_tag#v}" ;;
        *) fail "could not tell the latest release from ${l_url}; run the command again" ;;
    esac
}

installed_version() {
    [ -x "$binary" ] || return 0
    "$binary" --version </dev/null 2>/dev/null | sed -n 's/^simple-voice //p'
}

is_running() {
    pgrep -u "$(id -u)" -f "$pattern" >/dev/null 2>&1
}

# The app keeps nothing unsaved and its engine helper exits when the app's end of its stdin
# closes, so TERM is a clean quit.
quit_app() {
    pkill -TERM -u "$(id -u)" -f "$pattern" 2>/dev/null || true
    q_waited=0
    while is_running; do
        q_waited=$((q_waited + 1))
        if [ "$q_waited" -eq 20 ]; then
            pkill -KILL -u "$(id -u)" -f "$pattern" 2>/dev/null || true
        fi
        [ "$q_waited" -le 30 ] || fail "Simple Voice did not quit; quit it from its tray menu and run the command again"
        sleep 0.5
    done
}

# Starts the app detached from this script, in the desktop session the script runs in. Fails
# (status 1) without a desktop session to start it in.
reopen() {
    [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ] || return 1
    setsid -f "$binary" </dev/null >/dev/null 2>&1 || true
}

# The menu entry and icons from the unpacked release, pointed at this install.
install_desktop_entry() {
    e_tree="$1"
    mkdir -p "$(dirname "$desktop_file")"
    sed -e "s|^Exec=.*|Exec=\"${binary}\"|" -e "s|^Icon=.*|Icon=simple-voice|" \
        "${e_tree}/share/applications/simple-voice.desktop" >"$desktop_file"
    (cd "${e_tree}/share/icons/hicolor" && find . -type f -name 'simple-voice.*') |
        sed 's|^\./||' | while IFS= read -r e_icon; do
        mkdir -p "${icons}/$(dirname "$e_icon")"
        cp "${e_tree}/share/icons/hicolor/${e_icon}" "${icons}/${e_icon}"
    done
    update-desktop-database -q "$(dirname "$desktop_file")" </dev/null 2>/dev/null || true
    gtk-update-icon-cache -q -t "$icons" </dev/null 2>/dev/null || true
}

warn_system_package() {
    if is_installed_package "$package"; then
        note "The system package from an older installer is still there: sudo apt remove ${package}"
    fi
}

install_release() {
    i_version="$1"
    i_which="$2"
    case "$i_version" in
        "" | *[!0-9.]*) fail "not a release version: '${i_version}'; versions look like 0.4.0" ;;
    esac
    i_asset="Simple-Voice_${i_version}_amd64.tar.gz"
    if [ "$(installed_version)" = "$i_version" ]; then
        step "Linux x86-64 (AVX2):" "${i_asset}, ${i_which}"
        step "Simple Voice ${i_version} is already in $(pretty "$prefix")"
        return 0
    fi

    i_base="${SIMPLE_VOICE_RELEASES:-${releases_url}/download}/v${i_version}"
    i_tmp="${work}/download"
    mkdir "$i_tmp"
    # The small checksum file first: a missing release or no network fails here, with one line.
    fetch "${i_base}/${i_asset}.sha256" "${i_tmp}/${i_asset}.sha256" "release v${i_version}"
    step "Linux x86-64 (AVX2):" "${i_asset}, ${i_which}"
    if [ -t 2 ]; then
        curl_net -fL --progress-bar -o "${i_tmp}/${i_asset}" "${i_base}/${i_asset}" ||
            fail "the download of ${i_asset} broke off; run the command again"
    else
        fetch "${i_base}/${i_asset}" "${i_tmp}/${i_asset}" "${i_asset}"
    fi
    i_expected="$(awk 'NR == 1 { print $1 }' "${i_tmp}/${i_asset}.sha256")"
    i_actual="$(sha256sum "${i_tmp}/${i_asset}" | cut -d ' ' -f 1)"
    if [ -z "$i_expected" ] || [ "$i_actual" != "$i_expected" ]; then
        fail "checksum mismatch for ${i_asset} (expected ${i_expected:-nothing}, got ${i_actual}); nothing was installed, run the command again"
    fi
    tar -xzf "${i_tmp}/${i_asset}" -C "$i_tmp" </dev/null ||
        fail "${i_asset} is damaged; nothing was installed, run the command again"
    i_tree="${i_tmp}/${package}"
    if ! [ -x "${i_tree}/bin/simple-voice" ] || ! [ -x "${i_tree}/bin/simple-voice-engine" ]; then
        fail "${i_asset} is not the Simple Voice release; nothing was installed"
    fi
    step "Downloaded and verified" "(SHA-256 from ${i_asset}.sha256)"
    i_unresolved="$(LC_ALL=C ldd "${i_tree}/bin/simple-voice" "${i_tree}/bin/simple-voice-engine" 2>&1 |
        sed -n 's/^[[:space:]]*\([^ ]*\) => not found.*/\1/p' | sort -u | tr '\n' ' ')"
    [ -z "$i_unresolved" ] ||
        fail "this system lacks libraries Simple Voice needs (${i_unresolved% }); sudo apt install ${required_packages}, then run the command again"

    if is_running; then
        quit_app
        step "Quit the running Simple Voice to update it"
    fi
    mkdir -p "$(dirname "$prefix")" "$(dirname "$link")"
    rm -rf "${prefix}.new"
    mkdir "${prefix}.new"
    cp "${i_tree}/bin/simple-voice" "${i_tree}/bin/simple-voice-engine" "${prefix}.new/"
    rm -rf "${prefix}.old"
    if [ -e "$prefix" ]; then
        mv "$prefix" "${prefix}.old"
    fi
    mv "${prefix}.new" "$prefix"
    rm -rf "${prefix}.old"
    step "Installed $(pretty "$prefix")"
    ln -sfn "$binary" "$link"
    step "Linked $(pretty "$link")"
    install_desktop_entry "$i_tree"
    step "Added the menu entry and icons"
    warn_system_package
    case ":${PATH}:" in
        *":${HOME}/.local/bin:"*) ;;
        *) note "Add ~/.local/bin to your PATH to run simple-voice from a terminal" ;;
    esac
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
    if [ -e "$prefix" ]; then
        rm -rf "$prefix"
        step "Removed $(pretty "$prefix")"
    fi
    if [ -L "$link" ]; then
        rm -f "$link"
    fi
    rm -f "$desktop_file"
    find "$icons" -name 'simple-voice.*' -type f -delete 2>/dev/null || true
    # Development builds use ".dev" names and are left alone.
    rm -f "$autostart"
    rm -rf "${data_home:?}/${bundle_id}" "${XDG_CACHE_HOME:-${HOME}/.cache}/${bundle_id}"
    secret-tool clear service "${bundle_id}.groq" account groq-api-key </dev/null >/dev/null 2>&1 || true
    step "Removed the menu entry, the login entry, the web view's data and the saved Groq key"
    warn_system_package
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
    check_dependencies

    n_was_running=false
    if is_running; then
        n_was_running=true
    fi
    work="$(mktemp -d "${TMPDIR:-/tmp}/simple-voice.XXXXXX")"
    trap 'rm -rf "$work"' EXIT
    trap 'exit 130' HUP INT TERM

    if [ -z "$n_version" ]; then
        # Assigned on its own line so that a failure there stops the script (set -e).
        n_version="$(latest_version)"
        install_release "$n_version" "latest release"
    else
        install_release "$n_version" "release v${n_version}"
    fi

    if $n_was_running && ! is_running; then
        if reopen; then
            step "Reopened Simple Voice"
        else
            note "Open Simple Voice from your applications menu"
        fi
    fi

    printf '%s%s is installed.%s\n\n' "$bold" "$("$binary" --version </dev/null | sed 's/^simple-voice/Simple Voice/')" "$reset"
    printf 'Next:\n'
    printf '  Open Simple Voice from your applications menu (or run simple-voice), then press Ctrl+/ to dictate; GNOME on Wayland: see docs/linux.md for the shortcut\n'
}

main() {
    setup_colour
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
        check_platform
        uninstall "$a_purge"
        return 0
    fi
    if $a_purge; then
        fail "--purge only goes with --uninstall"
    fi
    install_main "$a_version"
}

main "$@"
