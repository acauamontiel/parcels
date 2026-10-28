#!/bin/sh
# Makes the test server download a multi-file torrent from the test peer, so
# transfers, peers and file progress show up in Parcels. Needs both services:
#
#   docker compose --profile test up -d qbittorrent qbittorrent-peer
#   build-aux/qbittorrent-test/seed-from-peer.sh [download limit in bytes/s]
set -eu

LIMIT=${1:-100000}
SERVER=http://localhost:8080
PEER=http://localhost:8081
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

login() {
    curl -sf -c "$TMP/$1.cookies" -H "Referer: $2" \
        --data 'username=admin&password=adminadmin' "$2/api/v2/auth/login" >/dev/null
}
server() { curl -sf -b "$TMP/server.cookies" -H "Referer: $SERVER" "$@"; }
peer() { curl -sf -b "$TMP/peer.cookies" -H "Referer: $PEER" "$@"; }

login server "$SERVER"
login peer "$PEER"

name="Blender Open Movies"
docker exec parcels-qbittorrent-peer sh -c "
    d='/downloads/$name'
    [ -d \"\$d\" ] && exit 0
    mkdir -p \"\$d/Sintel/Subtitles\" \"\$d/Big Buck Bunny\"
    head -c 40000000 /dev/urandom > \"\$d/Sintel/sintel-4k.mkv\"
    head -c 30000 /dev/urandom > \"\$d/Sintel/Subtitles/sintel-en.srt\"
    head -c 30000 /dev/urandom > \"\$d/Sintel/Subtitles/sintel-pt_BR.srt\"
    head -c 25000000 /dev/urandom > \"\$d/Big Buck Bunny/bbb-1080p.mp4\"
    head -c 2000 /dev/urandom > \"\$d/README.txt\"
    chown -R qbtUser:qbtUser /downloads"

hash() {
    python3 -c "import json,sys; print(next((t['hash'] for t in json.load(sys.stdin) if t['name'] == sys.argv[1]), ''))" "$name"
}

if [ -z "$(peer "$PEER/api/v2/torrents/info" | hash)" ]; then
    peer "$PEER/api/v2/torrentcreator/addTask" --data-urlencode "sourcePath=/downloads/$name" \
        --data 'startSeeding=true&trackers=http%3A%2F%2Ftracker.invalid%3A6969%2Fannounce' >/dev/null
    sleep 4
fi
h=$(peer "$PEER/api/v2/torrents/info" | hash)
peer "$PEER/api/v2/torrents/export?hash=$h" -o "$TMP/movies.torrent"

# Start over on the server, slowly enough to watch it download
server "$SERVER/api/v2/torrents/delete" --data "hashes=$h&deleteFiles=true" || true
sleep 1
server "$SERVER/api/v2/app/setPreferences" --data-urlencode 'json={"queueing_enabled":false}'
server "$SERVER/api/v2/transfer/setDownloadLimit" --data "limit=$LIMIT"
server -X POST "$SERVER/api/v2/torrents/add" -F "torrents=@$TMP/movies.torrent;type=application/x-bittorrent" \
    -F 'savepath=/downloads/peer' -F 'category=movies' -F 'tags=blender' >/dev/null
sleep 2
peer_ip=$(docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' parcels-qbittorrent-peer)
server "$SERVER/api/v2/torrents/addPeers" --data "hashes=$h&peers=$peer_ip:6882" >/dev/null
echo "Downloading “$name” ($h) from the peer at $LIMIT bytes/s"
