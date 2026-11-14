//! `roles/1` (`docs/protocol/capabilities/roles-1.md`): who leads on a link.
//!
//! Each side stores the mode from its own point of view; on the wire it is named from the point of
//! view of the message (`callerLeads` / `serverLeads`), so it can never be read backwards.

use serde::{Deserialize, Serialize};

/// How this device and one linked device stay in step, seen from this device.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    /// Changes go both ways (the default, Mehdi's D1).
    #[default]
    Both,
    /// This device leads: changes made on the other one are not taken.
    ILead,
    /// The other device leads: this one's changes stay here (D2).
    TheyLead,
}

/// The body of `GET /roles`, `PUT /roles` and their answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RolesWire {
    pub mode: String,
    pub mode_at: i64,
}

/// How a one-off copy combines the two libraries (`POST /roles/take`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum How {
    /// Nothing is removed; the sender's values win where both have one.
    Merge,
    /// The receiver's library becomes the sender's, after the receiver saves a backup of its own.
    Replace,
}

/// The body of `POST /roles/take`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TakeWire {
    pub how: How,
    pub request_id: String,
}

impl Mode {
    /// The wire name when this device answers (it is the server).
    pub fn as_server(self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::ILead => "serverLeads",
            Self::TheyLead => "callerLeads",
        }
    }

    /// The wire name when this device sends the message (it is the caller).
    pub fn as_caller(self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::ILead => "callerLeads",
            Self::TheyLead => "serverLeads",
        }
    }

    /// Reads a mode the other device sent in a request (this device is the server).
    pub fn from_request(wire: &str) -> Option<Self> {
        match wire {
            "both" => Some(Self::Both),
            "callerLeads" => Some(Self::TheyLead),
            "serverLeads" => Some(Self::ILead),
            _ => None,
        }
    }

    /// Reads a mode the other device answered with (this device was the caller).
    pub fn from_answer(wire: &str) -> Option<Self> {
        match wire {
            "both" => Some(Self::Both),
            "serverLeads" => Some(Self::TheyLead),
            "callerLeads" => Some(Self::ILead),
            _ => None,
        }
    }

    /// Whether this device applies changes made on the other one.
    pub fn takes_theirs(self) -> bool {
        self != Self::ILead
    }

    /// Whether this device sends the other one its own changes.
    pub fn sends_mine(self) -> bool {
        self != Self::TheyLead
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mode_survives_the_trip_in_both_directions() {
        for m in [Mode::Both, Mode::ILead, Mode::TheyLead] {
            // I send my mode; the other side reads it as a request and answers with its own view.
            let theirs = Mode::from_request(m.as_caller()).unwrap();
            assert_eq!(Mode::from_answer(theirs.as_server()), Some(m));
        }
        assert_eq!(Mode::from_request("callerLeads"), Some(Mode::TheyLead));
        assert_eq!(Mode::from_request("nonsense"), None);
        assert!(!Mode::ILead.takes_theirs() && Mode::ILead.sends_mine());
        assert!(Mode::TheyLead.takes_theirs() && !Mode::TheyLead.sends_mine());
    }
}
