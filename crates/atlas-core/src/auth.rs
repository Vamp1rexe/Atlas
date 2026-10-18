use anyhow::{bail, Result};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Account {
    pub name: String,
    pub uuid: String,
    pub access_token: String,
    pub user_type: String,
}

impl Account {
    /// Offline account. Works for singleplayer and offline-mode servers only.
    /// Microsoft login plugs in here later by returning an `Account` as well.
    pub fn offline(name: &str) -> Result<Self> {
        let name = name.trim();
        if !(3..=16).contains(&name.len())
            || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            bail!("Ungültiger Name: 3–16 Zeichen, nur A–Z, 0–9 und _");
        }
        Ok(Self {
            name: name.to_string(),
            uuid: offline_uuid(name),
            access_token: "0".to_string(),
            user_type: "legacy".to_string(),
        })
    }
}

/// Same UUID a vanilla server derives in offline mode (Java `nameUUIDFromBytes`).
pub fn offline_uuid(name: &str) -> String {
    let digest = Md5::digest(format!("OfflinePlayer:{name}").as_bytes());
    let mut b = [0u8; 16];
    b.copy_from_slice(&digest);
    b[6] = (b[6] & 0x0f) | 0x30;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h = hex::encode(b);
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_uuid_matches_vanilla() {
        // Known vanilla offline UUID for "Notch".
        assert_eq!(offline_uuid("Notch"), "b50ad385-829d-3141-a216-7e7d7539ba7f");
    }
}
