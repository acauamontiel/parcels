# Parcels

Remote control for qBittorrent, built for GNOME.

Parcels is a native GTK 4 and libadwaita app that manages a
[qBittorrent](https://www.qbittorrent.org/) instance running on another
machine (a home server, a NAS or a seedbox) through its WebUI API. It is meant
to stay open in the background and to handle `magnet:` links and `.torrent`
files for your desktop.

> [!NOTE]
> Parcels is in early development and is not affiliated with the qBittorrent project.

## Requirements

- qBittorrent 5.0 or later with the WebUI enabled
- For API key authentication: qBittorrent 5.2 or later

## Building

Parcels is built as a Flatpak with the GNOME 51 SDK. The development
environment runs in Docker, using the same image as CI, so you only need
Docker with Compose:

```sh
# Build the Devel Flatpak and run the test suite (cargo test, clippy, rustfmt, validators)
docker compose run --rm flatpak build-aux/dev.sh build

# Export a bundle and install it on your machine
docker compose run --rm flatpak build-aux/dev.sh bundle
flatpak install --user parcels-devel.flatpak
flatpak run br.com.acauamontiel.Parcels.Devel

# Lint the Flatpak manifest
docker compose run --rm flatpak build-aux/dev.sh lint
```

### Testing against a real server

`docker-compose.yml` has two throwaway qBittorrent instances for testing: a
server at <http://localhost:8080> and a peer at <http://localhost:8081>, both
with user `admin` and password `adminadmin`. Pick the
version with `QBT_VERSION` (an image tag of `qbittorrentofficial/qbittorrent-nox`):

```sh
QBT_VERSION=5.0.0-1 docker compose --profile test up -d qbittorrent qbittorrent-peer
```

To see a real transfer, with peers and file progress, make the server
download a torrent from the second test instance:

```sh
build-aux/qbittorrent-test/seed-from-peer.sh
```

The client crate has optional integration tests that run when `QBIT_URL` is
set, using `QBIT_USER` and `QBIT_PASS` and/or `QBIT_API_KEY`:

```sh
docker compose run --rm -e QBIT_URL=http://parcels-qbittorrent:8080 -e QBIT_USER=admin -e QBIT_PASS=adminadmin \
    flatpak build-aux/dev.sh cargo test -p qbittorrent-client --test live
```

Never commit real credentials.

You can also open the project in [GNOME Builder](https://apps.gnome.org/Builder/),
which picks up the Flatpak manifest in `build-aux/`.

## Translations

Parcels is translated with gettext. Translation files live in [`po/`](po/).

## License

Parcels is licensed under the [GNU General Public License v3.0 or later](COPYING).

## Credits

- Project structure based on
  [gtk-rust-template](https://gitlab.gnome.org/World/Rust/gtk-rust-template) by Bilal Elmoussaoui
  (MIT, see [`LICENSES/gtk-rust-template-MIT.md`](LICENSES/gtk-rust-template-MIT.md)).
- Many patterns are inspired by [Fragments](https://gitlab.gnome.org/World/Fragments)
  by Felix Häcker, a BitTorrent client for GNOME.
