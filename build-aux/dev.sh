#!/bin/sh
# Development helper. Meant to run inside the `flatpak` Docker Compose service:
#
#   docker compose run --rm flatpak build-aux/dev.sh <command>
#
# Commands:
#   build       Build the Devel Flatpak and run its tests (cargo test, clippy, fmt, validators)
#   bundle      Build and export parcels-devel.flatpak, installable on the host
#   lint        Run flatpak-builder-lint on the manifest
#   cargo ARGS  Run cargo inside the GNOME SDK (e.g. `cargo clippy --all-targets`)
#   screenshot  Start the built app in a headless X server and save a screenshot
set -eu

APP_ID=br.com.acauamontiel.Parcels.Devel
MANIFEST=build-aux/$APP_ID.json
FLATPAK_DIR=.flatpak
BUILD_DIR=$FLATPAK_DIR/build
REPO_DIR=$FLATPAK_DIR/repo

# D-Bus needs a machine ID, which minimal containers don't have
[ -s /etc/machine-id ] || dbus-uuidgen --ensure=/etc/machine-id

build() {
    flatpak-builder \
        --install-deps-from=flathub \
        --force-clean \
        --disable-rofiles-fuse \
        --install \
        --ccache \
        --state-dir="$FLATPAK_DIR/state" \
        --repo="$REPO_DIR" \
        "$BUILD_DIR" "$MANIFEST"
}

case "${1:-}" in
    build)
        build
        ;;
    bundle)
        build
        flatpak build-bundle --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
            "$REPO_DIR" parcels-devel.flatpak "$APP_ID"
        echo "Install on the host with: flatpak install --user parcels-devel.flatpak"
        ;;
    cargo)
        shift
        # Forward the optional integration test settings (crates/qbittorrent-client/tests/live.rs)
        set -- cargo "$@"
        for var in QBIT_URL QBIT_USER QBIT_PASS QBIT_API_KEY; do
            eval "value=\${$var:-}"
            [ -n "$value" ] && set -- "$var=$value" "$@"
        done
        flatpak run \
            --filesystem="$PWD" \
            --share=network \
            --env=CARGO_HOME="$PWD/$FLATPAK_DIR/cargo-home" \
            --env=CARGO_TARGET_DIR="$PWD/target" \
            --env=PATH=/usr/lib/sdk/rust-stable/bin:/usr/lib/sdk/llvm22/bin:/app/bin:/usr/bin \
            --env=HTTPS_PROXY="${HTTPS_PROXY:-}" \
            --cwd="$PWD" \
            --command=env \
            org.gnome.Sdk//51 "$@"
        ;;
    lint)
        flatpak-builder-lint manifest "$MANIFEST"
        ;;
    screenshot)
        build
        out=${2:-$FLATPAK_DIR/screenshot.png}
        fbdir=$(mktemp -d)
        Xvfb :99 -screen 0 1100x760x24 -fbdir "$fbdir" >/dev/null 2>&1 &
        xvfb_pid=$!
        sleep 1
        DISPLAY=:99 GDK_BACKEND=x11 dbus-run-session -- flatpak run "$APP_ID" >"$FLATPAK_DIR/app.log" 2>&1 &
        app_pid=$!
        sleep "${SCREENSHOT_DELAY:-8}"
        python3 build-aux/xwd2png.py "$fbdir/Xvfb_screen0" "$out"
        kill "$app_pid" "$xvfb_pid" 2>/dev/null || true
        echo "Screenshot saved to $out"
        ;;
    *)
        sed -n '2,14p' "$0"
        exit 1
        ;;
esac
