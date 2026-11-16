// identity.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Entity IDs use length-framed UTF-8 hashing, never a bus correlation counter.

use super::{Action, Fault, registry::route};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Content-addressed stored identity, distinct from action-id and instance counters.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct EntityId(String);

impl TryFrom<String> for EntityId {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let sizes = [12, 12, 12, 12, 16, 16, 16];
        let parts: Vec<_> = value.split('-').collect();
        if parts.len() != sizes.len()
            || parts.iter().zip(sizes).any(|(part, size)| {
                part.len() != size
                    || !part
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
        {
            return Err("invalid content-addressed entity identity");
        }
        Ok(Self(value))
    }
}

impl From<EntityId> for String {
    fn from(value: EntityId) -> Self {
        value.0
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

/// The only entity-ID mint; callers name action types, never genus strings.
pub struct IdGeneration;

impl IdGeneration {
    /// Hash length-prefixed genus/payload UTF-8 plus cryptographically random bytes.
    pub fn next_id<A: Action>(payload: &A::Payload) -> Result<EntityId, Fault> {
        let mut randomizer = [0_u8; 16];
        getrandom::fill(&mut randomizer).map_err(|_| {
            Fault::new(
                "entropy_unavailable",
                "Identity entropy is unavailable.",
                false,
            )
        })?;
        let genus = route::<A>();
        let payload = serde_json::to_vec(payload)
            .map_err(|_| Fault::new("invalid_payload", "Cannot encode action payload.", true))?;
        let mut digest = Sha256::new();
        for value in [genus.as_bytes(), payload.as_slice()] {
            let size = u16::try_from(value.len()).map_err(|_| {
                Fault::new(
                    "identity_input_too_large",
                    "Identity input exceeds 64 KiB.",
                    true,
                )
            })?;
            digest.update(size.to_be_bytes());
            digest.update(value);
        }
        digest.update(randomizer);
        let checksum = format!("{:x}", digest.finalize());
        let random = randomizer
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        Ok(EntityId(format!(
            "{}-{}-{}-{}-{}-{}-{}",
            &checksum[..12],
            &checksum[12..24],
            &checksum[24..36],
            &checksum[36..48],
            &checksum[48..],
            &random[..16],
            &random[16..],
        )))
    }
}
// end identity.rs
