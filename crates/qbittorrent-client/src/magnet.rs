use url::Url;

/// The parts of a magnet link Parcels shows before adding it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MagnetLink {
    /// The `dn` parameter, if present
    pub display_name: Option<String>,
    /// The info hash from `xt`, lowercase (hex or base32 as given)
    pub info_hash: String,
}

impl MagnetLink {
    /// Parses a BitTorrent magnet link (`xt=urn:btih:` or `urn:btmh:`).
    pub fn parse(text: &str) -> Option<Self> {
        let url = Url::parse(text.trim()).ok()?;
        if url.scheme() != "magnet" {
            return None;
        }

        let mut display_name = None;
        let mut info_hash = None;
        for (key, value) in url.query_pairs() {
            match key.as_ref() {
                "dn" if !value.is_empty() => display_name = Some(value.into_owned()),
                "xt" => {
                    let value = value.to_ascii_lowercase();
                    if let Some(hash) = value
                        .strip_prefix("urn:btih:")
                        .or_else(|| value.strip_prefix("urn:btmh:"))
                        && !hash.is_empty()
                    {
                        info_hash.get_or_insert_with(|| hash.to_owned());
                    }
                }
                _ => (),
            }
        }

        Some(Self {
            display_name,
            info_hash: info_hash?,
        })
    }
}

/// Whether `text` is something `torrents/add` accepts as a URL: a magnet
/// link or an HTTP(S) link to a `.torrent` file.
pub fn is_torrent_url(text: &str) -> bool {
    let text = text.trim();
    if MagnetLink::parse(text).is_some() {
        return true;
    }
    Url::parse(text).is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.has_host())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_magnets() {
        let magnet = MagnetLink::parse(
            "magnet:?xt=urn:btih:0123456789ABCDEF0123456789ABCDEF01234567&dn=Debian%2013%20DVD&tr=udp%3A%2F%2Ftracker",
        )
        .unwrap();
        assert_eq!(magnet.display_name.as_deref(), Some("Debian 13 DVD"));
        assert_eq!(magnet.info_hash, "0123456789abcdef0123456789abcdef01234567");

        let v2 = MagnetLink::parse("magnet:?xt=urn:btmh:1220abcd").unwrap();
        assert_eq!(v2.display_name, None);
    }

    #[test]
    fn rejects_non_torrent_magnets() {
        assert!(MagnetLink::parse("magnet:?xt=urn:ed2k:abc").is_none());
        assert!(MagnetLink::parse("magnet:?dn=no-hash").is_none());
        assert!(MagnetLink::parse("https://example.com/a.torrent").is_none());
        assert!(MagnetLink::parse("hello").is_none());
    }

    #[test]
    fn recognizes_torrent_urls() {
        assert!(is_torrent_url(" magnet:?xt=urn:btih:abc\n"));
        assert!(is_torrent_url("https://example.com/file.torrent"));
        assert!(!is_torrent_url("file:///tmp/a.torrent"));
        assert!(!is_torrent_url("just some text"));
    }
}
