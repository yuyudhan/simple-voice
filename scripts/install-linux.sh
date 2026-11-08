#!/usr/bin/env bash
# FilePath: scripts/install-linux.sh
# Installs, updates or uninstalls Simple Voice on Debian/Ubuntu Linux (x86-64) for the current
# user, without sudo. Published with every GitHub release beside install.sh, which hands over to
# it on Linux, so the README's one curl command works on both systems.
#
# It downloads the release's Simple-Voice_VERSION_amd64.tar.gz, checks it against the release's
# .sha256, and unpacks it into ~/.local/opt/simple-voice (the app and its engine helper side by
# side), links ~/.local/bin/simple-voice to it and adds the menu entry and icons under
# ~/.local/share. The system libraries the app links (WebKitGTK, GTK, the tray indicator) and
# the desktop tools it calls (xdotool, xclip, wl-clipboard, pactl, secret-tool) come from apt;
# this script never runs sudo: it checks for them and prints the one apt command to run when
# something is missing.
#
# It quits the app first and reopens it afterwards when it was running. Piping is safe: all work
# happens in main, called on the last line.
#
# Uninstalling quits the app and removes those files, the autostart entry, the webview's data and
# the Groq key in the keyring. The data in $XDG_DATA_HOME/simplevoice stays unless --purge is
# given.
#
# Options:
#   --version X.Y.Z    Install that release instead of the latest.
#   --uninstall        Remove the app; keep the data.
#   --purge            With --uninstall, also delete the data and a moved database.
#
# SIMPLE_VOICE_RELEASES replaces https://github.com/yuyudhan/simple-voice/releases/download, for
# testing a locally built release (file:///path, holding vVERSION/ with the assets); pass
# --version with it.
set -euo pipefail

repo="yuyudhan/simple-voice"
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
required_packages=(libwebkit2gtk-4.1-0 libgtk-3-0 libayatana-appindicator3-1)
tool_packages=(xdotool xclip wl-clipboard pulseaudio-utils libsecret-tools)

say() {
    printf 'simple-voice: %s\n' "$*"
}

fail() {
    printf 'simple-voice: %s\n' "$*" >&2
    exit 1
}

usage() {
    cat <<'EOF'
Install, update or uninstall Simple Voice on Linux, for your user only (no sudo).

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
    # whisper.cpp and ONNX Runtime are built for AVX2 and FMA.
    grep -q -w avx2 /proc/cpuinfo 2>/dev/null ||
        fail "Simple Voice for Linux needs a CPU with AVX2 (Intel Haswell, AMD Zen or later)"
}

is_installed_package() {
    dpkg-query -W -f='${Status}\n' "$1" 2>/dev/null | grep -q '^install ok installed$'
}

# The packages of "$@" that are not installed, space-separated. Without dpkg (not Debian or
# Ubuntu) nothing can be told, and the app's own library check below still runs.
missing_packages() {
    command -v dpkg-query >/dev/null 2>&1 || return 0
    local name missing=()
    for name in "$@"; do
        is_installed_package "$name" || missing+=("$name")
    done
    echo "${missing[*]}"
}

check_dependencies() {
    local libraries tools
    libraries=$(missing_packages "${required_packages[@]}")
    tools=$(missing_packages "${tool_packages[@]}")
    if [ -n "$libraries" ]; then
        say "Simple Voice needs these system libraries first. Install them, then run this again:"
        printf '\n    sudo apt install %s\n\n' "$libraries $tools" >&2
        exit 1
    fi
    if [ -n "$tools" ]; then
        say "optional tools are missing (pasting, selected text, muting, the Groq key store):"
        printf '\n    sudo apt install %s\n\n' "$tools"
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
    [ -x "$binary" ] || return 0
    "$binary" --version 2>/dev/null | sed -n 's/^simple-voice //p'
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

# The menu entry and icons from the unpacked release, pointed at this install.
install_desktop_entry() {
    local tree=$1 icon
    mkdir -p "$(dirname "$desktop_file")"
    sed -e "s|^Exec=.*|Exec=\"${binary}\"|" -e "s|^Icon=.*|Icon=simple-voice|" \
        "${tree}/share/applications/simple-voice.desktop" >"$desktop_file"
    while IFS= read -r icon; do
        mkdir -p "${icons}/$(dirname "$icon")"
        cp "${tree}/share/icons/hicolor/${icon}" "${icons}/${icon}"
    done < <(cd "${tree}/share/icons/hicolor" && find . -type f -name 'simple-voice.*' | sed 's|^\./||')
    update-desktop-database -q "$(dirname "$desktop_file")" 2>/dev/null || true
    gtk-update-icon-cache -q -t "$icons" 2>/dev/null || true
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
    asset="Simple-Voice_${version}_amd64.tar.gz"
    base="${SIMPLE_VOICE_RELEASES:-https://github.com/${repo}/releases/download}/v${version}"
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
    tar -xzf "${tmp}/${asset}" -C "$tmp" || fail "${asset} is damaged; nothing was installed"
    tree="${tmp}/${package}"
    if ! [ -x "${tree}/bin/simple-voice" ] || ! [ -x "${tree}/bin/simple-voice-engine" ]; then
        fail "${asset} is not the Simple Voice release; nothing was installed"
    fi
    unresolved=$(LC_ALL=C ldd "${tree}/bin/simple-voice" "${tree}/bin/simple-voice-engine" 2>&1 |
        sed -n 's/^[[:space:]]*\([^ ]*\) => not found.*/\1/p' | sort -u | tr '\n' ' ')
    [ -z "$unresolved" ] ||
        fail "this system lacks libraries Simple Voice needs (${unresolved% }); install: sudo apt install ${required_packages[*]}"

    if is_running; then
        quit_app
    fi
    mkdir -p "$(dirname "$prefix")" "$(dirname "$link")"
    rm -rf "${prefix}.new"
    mkdir "${prefix}.new"
    cp "${tree}/bin/simple-voice" "${tree}/bin/simple-voice-engine" "${prefix}.new/"
    rm -rf "${prefix}.old"
    if [ -e "$prefix" ]; then
        mv "$prefix" "${prefix}.old"
    fi
    mv "${prefix}.new" "$prefix"
    rm -rf "${prefix}.old"
    ln -sfn "$binary" "$link"
    install_desktop_entry "$tree"
    say "installed $("$binary" --version) in ${prefix}"
    if is_installed_package "$package"; then
        say "the system package from an older installer is still there; remove it with:"
        printf '\n    sudo apt remove %s\n\n' "$package"
    fi
    case ":${PATH}:" in
        *":${HOME}/.local/bin:"*) ;;
        *) say "add ~/.local/bin to your PATH to run simple-voice from a terminal" ;;
    esac
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
    if [ -e "$prefix" ]; then
        rm -rf "$prefix"
        say "removed ${prefix}"
    fi
    if [ -L "$link" ]; then
        rm -f "$link"
    fi
    rm -f "$desktop_file"
    find "$icons" -name 'simple-voice.*' -type f -delete 2>/dev/null || true
    # Development builds use ".dev" names and are left alone.
    rm -f "$autostart"
    rm -rf "${data_home:?}/${bundle_id}" "${XDG_CACHE_HOME:-${HOME}/.cache}/${bundle_id}"
    secret-tool clear service "${bundle_id}.groq" account groq-api-key >/dev/null 2>&1 || true
    say "removed the menu entry, the login entry, the web view's data and the saved Groq key"
    if is_installed_package "$package"; then
        say "the system package from an older installer is still there; remove it with:"
        printf '\n    sudo apt remove %s\n\n' "$package"
    fi
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
    check_dependencies

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
"simple-voice --toggle" (window managers can bind "simple-voice --start" and "--stop" for
hold-to-talk). Pasting under Wayland needs ydotool with access to /dev/uinput; without it the
text is copied and you press Ctrl+V. Details: docs/linux.md in the repository.
EOF
}

main "$@"
