use std::fmt;
use std::str::FromStr;

/// A `major.minor.patch` version, as reported by qBittorrent.
///
/// Accepts the formats the server uses, such as `v5.1.2`, `5.0.0beta1` or the
/// WebAPI version `2.11.4`. Missing components count as zero and any suffix
/// after the numbers is ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseVersionError(String);

impl fmt::Display for ParseVersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid version string: {:?}", self.0)
    }
}

impl std::error::Error for ParseVersionError {}

impl FromStr for Version {
    type Err = ParseVersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseVersionError(s.to_owned());
        let trimmed = s.trim();
        let trimmed = trimmed.strip_prefix(['v', 'V']).unwrap_or(trimmed);

        let mut parts = [0u32; 3];
        let mut count = 0;
        for (i, component) in trimmed.split('.').take(3).enumerate() {
            let digits: String = component
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if digits.is_empty() {
                break;
            }
            parts[i] = digits.parse().map_err(|_| err())?;
            count += 1;
            // A suffix such as "0beta1" ends the version
            if digits.len() != component.len() {
                break;
            }
        }

        if count == 0 {
            return Err(err());
        }

        Ok(Self::new(parts[0], parts[1], parts[2]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_server_formats() {
        assert_eq!("v5.1.2".parse(), Ok(Version::new(5, 1, 2)));
        assert_eq!("5.0.0beta1".parse(), Ok(Version::new(5, 0, 0)));
        assert_eq!("2.11.4".parse(), Ok(Version::new(2, 11, 4)));
        assert_eq!("v5.2".parse(), Ok(Version::new(5, 2, 0)));
        assert_eq!(" v4.6.7\n".parse(), Ok(Version::new(4, 6, 7)));
    }

    #[test]
    fn rejects_garbage() {
        assert!("".parse::<Version>().is_err());
        assert!("<html>".parse::<Version>().is_err());
        assert!("vX".parse::<Version>().is_err());
    }

    #[test]
    fn orders_numerically() {
        assert!(Version::new(5, 10, 0) > Version::new(5, 9, 9));
        assert!(Version::new(4, 6, 7) < crate::MIN_SERVER_VERSION);
    }
}
