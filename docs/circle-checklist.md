# GNOME Circle checklist

Status of Parcels against the [GNOME Circle app criteria](https://gitlab.gnome.org/Teams/Circle/-/blob/main/README.md)
and the [Flathub requirements](https://docs.flathub.org/docs/for-app-authors/requirements),
which Circle builds on.

Legend: ✅ done · ⚠️ partly done or needs checking · ❌ missing

| Area | Status | Notes |
|---|---|---|
| Free software license | ✅ | GPL-3.0-or-later (`COPYING`); template code is MIT, credited in `LICENSES/` |
| Code of conduct | ✅ | GNOME Code of Conduct, in `CODE_OF_CONDUCT.md` |
| Builds with the current GNOME runtime | ✅ | GNOME 51, GTK 4.24, libadwaita 1.10 |
| Published on Flathub | ❌ | Release manifest ready (`build-aux/br.com.acauamontiel.Parcels.json`); see "Before submitting" |
| Offline build with vendored dependencies | ✅ | `build-aux/cargo-sources.json` (flatpak-cargo-generator); checked in CI |
| Sandbox permissions | ✅ | Only network, IPC, Wayland/X11 fallback and DRI. No filesystem access: files come through portals |
| Secrets | ✅ | Passwords and API keys in the keyring (oo7: Secret portal or Secret Service), never in GSettings |
| No telemetry or tracking | ✅ | The only network traffic goes to the configured servers |
| Name and branding | ✅ | "Parcels" doesn't use the qBittorrent trademark; "not affiliated" is stated in the metainfo and About |
| Application icon | ❌ | Placeholder. Needs a design following the [GNOME app icon guidelines](https://developer.gnome.org/hig/guidelines/app-icons.html), plus a matching symbolic icon |
| AppStream metainfo | ⚠️ | Complete and valid with `appstreamcli validate --pedantic`. Screenshot URLs point to `main`, which doesn't exist yet |
| Screenshots | ⚠️ | In `data/screenshots/`, taken from the release build with the default style. Retake them once the icon is final |
| Brand colors | ⚠️ | Set, but chosen for the placeholder icon |
| Release notes | ✅ | In the metainfo, shown by GNOME Software and the About dialog |
| Human Interface Guidelines | ⚠️ | libadwaita patterns throughout (split view, navigation view, dialogs, toasts, banners, preferences). Needs a review by a GNOME designer |
| Adaptive layout | ✅ | Usable from 360×294 px: sidebar collapses below 640 sp, details tabs move to a bottom bar below 550 sp |
| Dark style and high contrast | ✅ | No hardcoded colors; the only CSS uses libadwaita variables |
| Keyboard navigation | ✅ | Every action is reachable by keyboard: selection bar, context menu (Shift+F10/Menu), standard shortcuts (Ctrl+Q, Ctrl+W, Ctrl+,, Ctrl+F, Ctrl+O, Ctrl+V, Delete), shortcuts dialog |
| Accessibility | ⚠️ | Icon-only buttons have tooltips/labels, progress bars and filters have accessible labels, results are announced. Not yet tested with Orca |
| Translations | ⚠️ | gettext from the start, complete pt_BR; `check-i18n` test catches missing strings. Needs a translation platform (e.g. Weblate) to invite translators |
| Error handling | ✅ | Network and API errors are shown in plain language; no panics on network or parse errors |
| Documentation | ✅ | README with build and test instructions |
| Maintenance | ⚠️ | Active development. Circle expects the app to be maintained after acceptance |
| Desktop integration | ✅ | Handler for `magnet:` and `.torrent`, single instance, drag and drop, background portal, notifications |

## Before submitting to Flathub

1. Replace the placeholder icon (and retake the screenshots with it).
2. Merge into `main` so the screenshot URLs in the metainfo resolve.
3. Tag `v0.1.0` and add its commit hash to the git source of the release manifest
   (the linter warns about `module-parcels-source-git-no-commit-with-tag` until then).
4. Verify the `acauamontiel.com.br` domain on Flathub, as the app ID is `br.com.acauamontiel.Parcels`.
5. Open the submission against [flathub/flathub](https://github.com/flathub/flathub) with the
   release manifest and `cargo-sources.json`.

## Checked locally

- `flatpak-builder-lint manifest` on both manifests
- `flatpak-builder-lint repo` on the release build. Two errors remain, both about
  screenshots: `appstream-missing-screenshots` (their URLs point to `main`, which doesn't
  exist yet) and `appstream-screenshots-not-mirrored-in-ostree` (only Flathub's own
  infrastructure mirrors screenshots)
- `appstreamcli validate --pedantic`, `desktop-file-validate` and
  `glib-compile-schemas --strict` (Meson tests)
- `cargo fmt --check` and `cargo clippy -- -D warnings` (Meson tests in the Devel profile)

## Still to test on a real GNOME session

These depend on desktop services that the development container doesn't have:

- Keyring: saving and reading passwords through the Secret portal
- Notifications as shown by GNOME Shell, and clicking them
- Background portal prompts, autostart after logging in
- Opening magnet links from a browser and `.torrent` files from Files
- Orca with the main window, dialogs and the details page
