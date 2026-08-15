//! The SSE-session proof (hand-written; user-owned).
//!
//! ADR-0018 Tier-A capability shape: a bearer token minted at SSE-connect
//! time, bound to ONE identity and an expiry, signed
//! `HMAC-SHA256(secret, identity_id|expiry)`. It is the port of Odoo's
//! `is_websocket_session` check — the ONLY thing that authorizes
//! `update_bus_presence` (a client proves it holds a live stream by holding
//! the proof minted when that stream opened).
//!
//! The token is `base64url(identity_id).base64url(exp_unix_secs).base64url(sig)`.
//! Verification is constant-time on the signature bytes.

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::application::service::chatter_acl::MessagingIdentity;

type HmacSha256 = Hmac<Sha256>;

/// Default proof lifetime (matches the SSE keepalive cadence budget).
pub const SESSION_TTL_SECS: i64 = 3600;

/// Mint a session proof for an identity (called at SSE connect).
pub fn mint_session_token(secret: &[u8], identity: &MessagingIdentity, ttl_secs: i64) -> String {
    let id = identity_key(identity);
    let exp = chrono::Utc::now().timestamp() + ttl_secs;
    let payload = format!("{id}|{exp}");
    let sig = mac(secret, payload.as_bytes());
    format!("{}.{}.{}", b64(id.as_bytes()), b64(exp.to_string().as_bytes()), b64(&sig))
}

/// Verify a proof against the identity that claims it. False on ANY
/// malformed input, expiry, identity mismatch, or signature mismatch.
pub fn verify_session_token(secret: &[u8], token: &str, identity: &MessagingIdentity) -> bool {
    let mut parts = token.split('.');
    let (Some(id_b64), Some(exp_b64), Some(sig_b64), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let (Ok(id_raw), Ok(exp_raw), Ok(sig_given)) =
        (ub64(id_b64), ub64(exp_b64), ub64(sig_b64))
    else {
        return false;
    };
    let (Ok(id), Ok(exp)) = (String::from_utf8(id_raw), String::from_utf8(exp_raw)) else {
        return false;
    };
    if id != identity_key(identity) {
        return false;
    }
    let Ok(exp) = exp.parse::<i64>() else { return false };
    if chrono::Utc::now().timestamp() > exp {
        return false;
    }
    let payload = format!("{id}|{exp}");
    let expected = mac(secret, payload.as_bytes());
    const_eq(&expected, &sig_given)
}

/// The stable per-identity key (`p:<uuid>` / `g:<uuid>`).
fn identity_key(identity: &MessagingIdentity) -> String {
    match identity {
        MessagingIdentity::User { partner_id } => format!("p:{partner_id}"),
        MessagingIdentity::Guest { guest_id } => format!("g:{guest_id}"),
    }
}

fn mac(secret: &[u8], payload: &[u8]) -> [u8; 32] {
    let mut m = HmacSha256::new_from_slice(secret)
        .unwrap_or_else(|_| unreachable!("HMAC-SHA256 accepts any key length"));
    m.update(payload);
    m.finalize().into_bytes().into()
}

fn const_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn b64(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn ub64(s: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn user() -> MessagingIdentity {
        MessagingIdentity::User { partner_id: Uuid::new_v4() }
    }

    #[test]
    fn roundtrip_and_identity_binding() {
        let id = user();
        let tok = mint_session_token(b"s", &id, 60);
        assert!(verify_session_token(b"s", &tok, &id));
        // A different identity cannot use it.
        assert!(!verify_session_token(b"s", &tok, &user()));
        // Nor a different secret.
        assert!(!verify_session_token(b"t", &tok, &id));
    }

    #[test]
    fn expiry_enforced() {
        let id = user();
        let tok = mint_session_token(b"s", &id, -1); // already expired
        assert!(!verify_session_token(b"s", &tok, &id));
    }

    #[test]
    fn malformed_tokens_fail_closed() {
        let id = user();
        for bad in ["", "a", "a.b", "a.b.c.d", "!!!.?.#"] {
            assert!(!verify_session_token(b"s", bad, &id), "{bad:?}");
        }
        // Tampered signature segment.
        let tok = mint_session_token(b"s", &id, 60);
        let mut parts = tok.split('.').collect::<Vec<_>>();
        parts[2] = "AAAA";
        assert!(!verify_session_token(b"s", &parts.join("."), &id));
    }
}
