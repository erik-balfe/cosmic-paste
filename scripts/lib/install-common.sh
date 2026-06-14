# Shared install steps for cosmic-paste (source and release bundles).
# shellcheck shell=bash
# Not executable on its own — source from install.sh / install-release.sh.

cosmic_paste_need() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "error: required command not found: $1" >&2
        exit 1
    }
}

cosmic_paste_detect_os() {
    if [[ -r /etc/os-release ]]; then
        # shellcheck disable=SC1091
        . /etc/os-release
        echo "${ID:-unknown}"
        return
    fi
    echo "unknown"
}

cosmic_paste_is_cosmic_session() {
    case "${XDG_CURRENT_DESKTOP:-}${XDG_SESSION_DESKTOP:-}" in
        *COSMIC* | *cosmic*) return 0 ;;
    esac
    return 1
}

# Runtime packages for prebuilt binaries (no Rust toolchain).
cosmic_paste_install_runtime_deps() {
    if command -v wl-copy >/dev/null && command -v wl-paste >/dev/null; then
        : # wl-clipboard OK
    else
        echo "==> Installing wl-clipboard (required for paste)..."
        case "$(cosmic_paste_detect_os)" in
            fedora)
                sudo dnf install -y wl-clipboard
                ;;
            ubuntu | pop | debian)
                sudo apt-get update
                sudo apt-get install -y wl-clipboard
                ;;
            arch | endeavouros | manjaro)
                sudo pacman -Sy --needed --noconfirm wl-clipboard
                ;;
            *)
                echo "error: wl-clipboard not found. Install wl-copy/wl-paste for your distro, then re-run." >&2
                exit 1
                ;;
        esac
    fi

    # Applet links libxkbcommon at runtime.
    if ! ldconfig -p 2>/dev/null | grep -q 'libxkbcommon\.so'; then
        echo "==> Installing libxkbcommon (required for panel applet)..."
        case "$(cosmic_paste_detect_os)" in
            fedora)
                sudo dnf install -y libxkbcommon
                ;;
            ubuntu | pop | debian)
                sudo apt-get install -y libxkbcommon0
                ;;
            arch | endeavouros | manjaro)
                sudo pacman -Sy --needed --noconfirm libxkbcommon
                ;;
            *)
                echo "warning: libxkbcommon may be missing — panel applet might not start" >&2
                ;;
        esac
    fi
}

cosmic_paste_install_build_deps() {
    cosmic_paste_install_runtime_deps
    case "$(cosmic_paste_detect_os)" in
        fedora)
            sudo dnf install -y gcc pkgconf-pkg-config libxkbcommon-devel wayland-devel
            ;;
        ubuntu | pop | debian)
            sudo apt-get update
            sudo apt-get install -y build-essential pkg-config libxkbcommon-dev libwayland-dev
            ;;
        arch | endeavouros | manjaro)
            sudo pacman -Sy --needed --noconfirm base-devel pkgconf libxkbcommon wayland
            ;;
        *)
            echo "warning: install Rust, pkg-config, libxkbcommon-devel, wayland-devel manually" >&2
            ;;
    esac
}

cosmic_paste_read_version() {
    local root="$1"
    if [[ -f "${root}/VERSION" ]]; then
        tr -d '[:space:]' < "${root}/VERSION"
        return
    fi
    if [[ -f "${root}/Cargo.toml" ]]; then
        grep '^version' "${root}/Cargo.toml" | head -1 | awk '{print $3}' | tr -d '"'
        return
    fi
    echo "unknown"
}

# Install binaries + user units + desktop entry + prepared shortcut example.
# Arguments: ROOT (data tree), BINDIR, BIN_SRC_DIR (directory with executables)
cosmic_paste_install_files() {
    local root="$1"
    local bindir="$2"
    local bin_src="$3"
    local share_dir="${HOME}/.local/share/cosmic-paste"
    local shortcuts_out="${share_dir}/custom-shortcuts.ron"

    cosmic_paste_need systemctl

    mkdir -p \
        "${HOME}/.config/systemd/user" \
        "${HOME}/.local/share/dbus-1/services" \
        "${HOME}/.local/share/applications" \
        "${HOME}/.local/share/icons/hicolor/scalable/apps" \
        "${bindir}" \
        "${share_dir}"

    install -m 0755 \
        "${bin_src}/cosmic-paste-daemon" \
        "${bin_src}/cosmic-paste" \
        "${bin_src}/cosmic-paste-applet" \
        "${bin_src}/cosmic-paste-show-history" \
        "${bindir}/"

    sed "s|@bindir@|${bindir}|g" "${root}/data/systemd/com.system76.CosmicPaste.service" \
        > "${HOME}/.config/systemd/user/com.system76.CosmicPaste.service"

    sed "s|@bindir@|${bindir}|g" "${root}/data/dbus/org.system76.CosmicPaste.service" \
        > "${HOME}/.local/share/dbus-1/services/org.system76.CosmicPaste.service"

    sed "s|@bindir@|${bindir}|g" "${root}/data/com.system76.CosmicPaste.Applet.desktop" \
        > "${HOME}/.local/share/applications/com.system76.CosmicPaste.Applet.desktop"

    local icon_src="${root}/share/icons/com.system76.CosmicPaste.Applet-symbolic.svg"
    if [[ ! -f "${icon_src}" ]]; then
        icon_src="${root}/cosmic-paste-applet/icons/paste-symbolic.svg"
    fi
    install -m 0644 "${icon_src}" \
        "${HOME}/.local/share/icons/hicolor/scalable/apps/com.system76.CosmicPaste.Applet-symbolic.svg"

    sed "s|@bindir@|${bindir}|g" "${root}/data/examples/cosmic-custom-shortcuts.ron" \
        > "${shortcuts_out}"

    rm -f \
        "${HOME}/.local/share/dbus-1/services/com.system76.CosmicPaste.service" \
        "${HOME}/.local/share/dbus-1/services/com.system76.CosmicPaste.Applet.service"
}

cosmic_paste_enable_daemon() {
    pkill -x cosmic-paste-applet 2>/dev/null || true
    systemctl --user daemon-reload
    systemctl --user enable --now com.system76.CosmicPaste.service
}

cosmic_paste_print_finish() {
    local version="$1"
    local shortcuts_file="${HOME}/.local/share/cosmic-paste/custom-shortcuts.ron"
    local portal="unknown"
    if busctl --user get-property org.system76.CosmicPaste /org/system76/CosmicPaste \
        org.system76.CosmicPaste2 PortalShortcutsAvailable 2>/dev/null | grep -q 'true'; then
        portal="true"
    else
        portal="false"
    fi

    echo ""
    echo "══════════════════════════════════════════════════════════════"
    echo "  cosmic-paste ${version} — installed"
    echo "══════════════════════════════════════════════════════════════"
    echo ""
    echo "Automatic:"
    echo "  • Binaries in ~/.local/bin"
    echo "  • Clipboard daemon enabled and running"
    echo "  • Panel applet registered (desktop entry + icon)"
    echo ""
    echo "Do once in COSMIC Settings (GUI):"
    echo "  1. Desktop → Panel → Applets → End segment → COSMIC Paste"
    echo "     If the icon is missing: killall cosmic-panel"
    echo ""
    if [[ "${portal}" == "false" ]]; then
        echo "  2. Keyboard → Custom shortcuts — add entries from:"
        echo "     ${shortcuts_file}"
        echo "     (Ctrl+F9 newer, Ctrl+F10 older, Ctrl+F11 popup)"
        echo ""
    else
        echo "  2. Shortcuts: Ctrl+F9 newer, Ctrl+F10 older, Ctrl+F11 popup (portal active)"
        echo ""
    fi
    echo "CLI: cosmic-paste history | prev | next | show-history"
    echo ""
    if ! cosmic_paste_is_cosmic_session; then
        echo "note: COSMIC desktop session not detected — this app targets COSMIC on Wayland."
        echo ""
    fi
}