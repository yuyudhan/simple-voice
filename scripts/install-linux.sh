#!/usr/bin/env bash
# FilePath: scripts/install-linux.sh
# Installs, updates or uninstalls Simple Voice on Debian/Ubuntu Linux (x86-64). Published with
# every GitHub release beside install.sh, which hands over to it on Linux, so the README's one
# curl command works on both systems. It installs the release's .deb through apt (with sudo),
# which also brings the libraries and desktop tools the app uses (WebKitGTK, the tray
# indicator, xdotool, xclip, wl-clipboard, pactl, secret-tool).
#
# It checks the .deb against the release's .sha256 and its package name before installing,
# quits the app first and reopens it afterwards when it was running. Piping is safe: all work
# happens in main, called on the last line.
#
# Uninstalling quits the app and removes the package, the autostart entry, the webview's data
# and the Groq key in the keyring. The data in $XDG_DATA_HOME/simplevoice stays unless --purge
# is given.
#
# Options:
#   --version X.Y.Z    Install that release instead of the latest.
#   --uninstall        Remove the app; keep the data.
#   --purge            With --uninstall, also delete the data and a moved database.
set -euo pipefail

repo="yuyudhan/simple-voice"
bundle_id="dev.yuyudhan.simplevoice"
package="simple-voice"
binary="/usr/bin/simple-voice"
pattern="^${binary}( |$)"
data_dir="${XDG_DATA_HOME:-${HOME}/.local/share}/simplevoice"
autostart="${XDG_CONFIG_HOME:-${HOME}/.config}/autostart/${bundle_id}.desktop"
database_file="simple-voice.db"

say() {
    printf 'simple-voice: %s\n' "$*"
}

fail() {
    printf 'simple-voice: %s\n' "$*" >&2
    exit 1
}

usage() {
    cat <<'EOF'
Install, update or uninstall Simple Voice on Linux.

usage: bash -s -- [options]   (piped), or bash install-linux.sh [options]

  --version X.Y.Z    Install that release instead of the latest.
  --uninstall        Remove the app; keep the data in ~/.local/share/simplevoice.
  --purge            With --uninstall, also delete that data (history, dictionary, settings,
                     models) and the database in a folder it was moved to.
EOF
}

check_platform() {
    [ "$(uname -s)" = "Linux" ] || fail "this installer is for Linux; use install.sh on macOS"
    [ "$(uname -m)" = "x86_64" ] || fail "Simple Voice for Linux needs an x86-64 computer"
    if ! command -v apt-get >/dev/null 2>&1 || ! command -v dpkg-query >/dev/null 2>&1; then
        fail "Simple Voice for Linux installs with apt (Debian, Ubuntu and their derivatives)"
    fi
    # whisper.cpp is built for AVX2 and FMA.
    grep -q -w avx2 /proc/cpuinfo 2>/dev/null ||
        fail "Simple Voice for Linux needs a CPU with AVX2 (Intel Haswell, AMD Zen or later)"
}

# apt needs root. sudo asks on the terminal even when this script is piped; from the app's
# Install button there is no terminal, so the user is told to run the command themselves.
as_root() {
    if [ "$(id -u)" -eq 0 ]; then
        "$@"
    elif sudo -n true 2>/dev/null || [ -t 2 ]; then
        sudo "$@"
    else
        fail "installing needs your password: run the install command from README.md in a terminal"
    fi
}

latest_version() {
    # github.com/<repo>/releases/latest redirects to the newest tag; no API token or JSON needed.
    url=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/${repo}/releases/latest") ||
        fail "could not reach GitHub to find the latest release"
    tag=${url##*/}
    echo "${tag#v}"
}

installed_version() {
    dpkg-query -W -f='${Status} ${Version}\n' "$package" 2>/dev/null |
        sed -n 's/^install ok installed //p'
}

is_running() {
    pgrep -u "$(id -u)" -f "$pattern" >/dev/null 2>&1
}

# The app keeps nothing unsaved and its engine helper exits when the app's end of its stdin
# closes, so TERM is a clean quit.
quit_app() {
    say "quitting Simple Voice"
    pkill -TERM -u "$(id -u)" -f "$pattern" 2>/dev/null || true
    waited=0
    while is_running; do
        waited=$((waited + 1))
        if [ "$waited" -eq 20 ]; then
            pkill -KILL -u "$(id -u)" -f "$pattern" 2>/dev/null || true
        fi
        [ "$waited" -le 30 ] || fail "Simple Voice did not quit; quit it from its tray menu and install again"
        sleep 0.5
    done
}

# Starts the app detached from this script, in the desktop session the script runs in.
reopen() {
    if [ -z "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]; then
        say "open Simple Voice from your applications menu"
        return 0
    fi
    setsid -f "$binary" </dev/null >/dev/null 2>&1 || true
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
    asset="Simple-Voice_${version}_amd64.deb"
    base="https://github.com/${repo}/releases/download/v${version}"
    say "downloading Simple Voice ${version}"
    if [ -t 2 ]; then
        progress=--progress-bar
    else
        progress=--silent
    fi
    curl -fL --show-error "$progress" -o "${tmp}/${asset}" "${base}/${asset}" ||
        fail "download failed: ${base}/${asset}"
    curl -fsSL -o "${tmp}/${asset}.sha256" "${base}/${asset}.sha256" ||
        fail "download failed: ${base}/${asset}.sha256"
    (cd "$tmp" && sha256sum -c "${asset}.sha256" >/dev/null) ||
        fail "checksum mismatch for ${asset}; nothing was installed"
    [ "$(dpkg-deb -f "${tmp}/${asset}" Package 2>/dev/null)" = "$package" ] ||
        fail "${asset} is not the Simple Voice package; nothing was installed"
    # apt reads local packages as its unprivileged _apt user.
    chmod 755 "$work" "$tmp"
    chmod 644 "${tmp}/${asset}"

    if is_running; then
        quit_app
    fi
    # A new machine may need a fresh package index for the dependencies.
    if ! as_root apt-get install -y -q "${tmp}/${asset}"; then
        as_root apt-get update -q
        as_root apt-get install -y -q "${tmp}/${asset}" || fail "apt could not install ${asset}"
    fi
    say "installed $("$binary" --version)"
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
    local purge=$1
    if is_running; then
        quit_app
    fi
    if [ -n "$(installed_version)" ]; then
        as_root apt-get remove -y -q "$package"
        say "removed the ${package} package"
    fi
    # Development builds use ".dev" names and are left alone.
    rm -f "$autostart"
    rm -rf "${XDG_DATA_HOME:-${HOME}/.local/share}/${bundle_id}" \
        "${XDG_CACHE_HOME:-${HOME}/.cache}/${bundle_id}"
    secret-tool clear service "${bundle_id}.groq" account groq-api-key >/dev/null 2>&1 || true
    say "removed the login entry, the web view's data and the saved Groq key"
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
                [ "$#" -ge 2 ] || fail "--version needs a value such as 0.1.3"
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

    check_platform
    if [ "$action" = "uninstall" ]; then
        [ -z "$version" ] || fail "--version and --uninstall cannot be combined"
        uninstall "$purge"
        return 0
    fi
    if $purge; then
        fail "--purge only goes with --uninstall"
    fi

    was_running=false
    if is_running; then
        was_running=true
    fi
    work=$(mktemp -d "${TMPDIR:-/tmp}/simple-voice.XXXXXX")
    trap 'rm -rf "$work"' EXIT
    trap 'exit 130' INT TERM

    if [ -z "$version" ]; then
        version=$(latest_version)
    fi
    install_release "$version"

    if $was_running && ! is_running; then
        reopen
    elif ! is_running; then
        say "open Simple Voice from your applications menu, or run: simple-voice"
    fi

    cat <<'EOF'

Dictate with Ctrl+/ (tap to start and stop, hold to talk). GNOME on Wayland gives apps no
global shortcuts: add one in Settings > Keyboard > Custom Shortcuts with the command
"simple-voice --toggle". Pasting under Wayland needs ydotool with access to /dev/uinput;
without it the text is copied and you press Ctrl+V. Details: docs/linux.md in the repository.
EOF
}

main "$@"
