use serde::{Deserialize, Serialize};

/// How a [`ClockTarget`] was found (`discovery.Clock.source`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClockSource {
    Mdns,
    Scan,
    Saved,
    Manual,
}

impl ClockSource {
    /// The Python spelling (`"mdns" | "scan" | "saved" | "manual"`).
    pub fn as_str(self) -> &'static str {
        match self {
            ClockSource::Mdns => "mdns",
            ClockSource::Scan => "scan",
            ClockSource::Saved => "saved",
            ClockSource::Manual => "manual",
        }
    }
}

/// `discovery.Clock`: one Clawd clock on the LAN.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClockTarget {
    pub host: String,
    /// HTTP port; 80 unless typed or advertised otherwise.
    pub port: u16,
    /// mDNS id (the clock's hostname); empty when typed by hand.
    pub name: String,
    pub version: String,
    pub source: ClockSource,
}

impl ClockTarget {
    /// A clock at `host:80`, no name or version, found by `source`.
    pub fn new(host: impl Into<String>, source: ClockSource) -> Self {
        Self {
            host: host.into(),
            port: 80,
            name: String::new(),
            version: String::new(),
            source,
        }
    }

    /// `http://host` (the port only when it is not 80).
    pub fn base(&self) -> String {
        if self.port == 80 {
            format!("http://{}", self.host)
        } else {
            format!("http://{}:{}", self.host, self.port)
        }
    }

    /// `<base>/api/usage`.
    pub fn usage_url(&self) -> String {
        format!("{}{}", self.base(), crate::consts::HTTP_PUSH_PATH)
    }

    /// `name (host)`, or just `host` when it has no name.
    pub fn label(&self) -> String {
        if self.name.is_empty() {
            self.host.clone()
        } else {
            format!("{} ({})", self.name, self.host)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_and_label() {
        let mut c = ClockTarget::new("10.0.0.5", ClockSource::Manual);
        assert_eq!(c.base(), "http://10.0.0.5");
        assert_eq!(c.usage_url(), "http://10.0.0.5/api/usage");
        assert_eq!(c.label(), "10.0.0.5");
        c.port = 8080;
        c.name = "clawd".into();
        assert_eq!(c.base(), "http://10.0.0.5:8080");
        assert_eq!(c.label(), "clawd (10.0.0.5)");
        assert_eq!(serde_json::to_value(c.source).unwrap(), "manual");
    }
}
