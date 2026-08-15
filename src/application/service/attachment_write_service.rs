//! The attachment write service (hand-written; user-owned).
//!
//! Port of the mail attachment surface (MAIL-M45, slim — the full
//! `ir.attachment` port stays with the system module): register an uploaded
//! blob's metadata (the `datas` column holds an opaque STORAGE HANDLE, never
//! base64 bytes — the object store is increment-3 territory; this layer only
//! records the handle), attach/detach on messages with an ownership gate, and
//! mint/clear the public access token.
//!
//! The token's authz semantics are Odoo's: possession IS the grant. The token
//! is compared by consteq at the READ boundary (the route layer, Stage 3) —
//! it is never a DB lookup key for authorization.

use uuid::Uuid;

use crate::application::service::chatter_acl::MessagingIdentity;
use crate::domain::event::constants::stage_bus_event;
use crate::infrastructure::persistence::attachment_repository::AttachmentRepository;

#[derive(Debug, thiserror::Error)]
pub enum AttachmentError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("not found: attachment {0}")]
    NotFound(Uuid),
    #[error("forbidden: attachment {0} is not yours")]
    Forbidden(Uuid),
    #[error("an attachment needs an owner (partner or guest)")]
    NeedsOwner,
}

pub struct AttachmentWriteService {
    pool: sqlx::PgPool,
}

impl AttachmentWriteService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Register an uploaded attachment's metadata. `datas` is the storage
    /// handle the upload path minted (the blob itself never passes through
    /// here). Owner is the caller's identity — XOR by construction.
    pub async fn register(
        &self,
        identity: &MessagingIdentity,
        name: &str,
        mimetype: Option<&str>,
        size: Option<i32>,
        datas: Option<&str>,
        checksum: Option<&str>,
    ) -> Result<Uuid, AttachmentError> {
        if name.trim().is_empty() || name.len() > 255 {
            return Err(AttachmentError::Invalid("name must be 1..=255 chars".into()));
        }
        if let Some(size) = size {
            if size < 0 {
                return Err(AttachmentError::Invalid("size cannot be negative".into()));
            }
        }
        let (owner_party_id, owner_guest_id) = match identity {
            MessagingIdentity::User { partner_id } => (Some(*partner_id), None),
            MessagingIdentity::Guest { guest_id } => (None, Some(*guest_id)),
        };
        if owner_party_id.is_none() && owner_guest_id.is_none() {
            return Err(AttachmentError::NeedsOwner);
        }

        let id = Uuid::new_v4();
        let mut tx = self.pool.begin().await?;
        AttachmentRepository::insert_attachment(
            &mut tx, id, name, mimetype, size, datas, checksum, owner_party_id, owner_guest_id,
        )
        .await?;
        stage_bus_event(
            &mut tx,
            "AttachmentCreated",
            "MailAttachment",
            id,
            identity.channel(),
            "mail.attachment/insert",
            serde_json::json!({
                "attachment_id": id,
                "name": name,
                "size": size,
                "owner_party_id": owner_party_id,
                "owner_guest_id": owner_guest_id,
            }),
        )
        .await?;
        tx.commit().await?;
        Ok(id)
    }

    /// Attach an attachment to a message (owner-gated: the uploader or an
    /// admin route). Idempotent. Returns true when a new join row landed.
    pub async fn attach_to_message(
        &self,
        identity: &MessagingIdentity,
        is_admin: bool,
        message_id: Uuid,
        attachment_id: Uuid,
    ) -> Result<bool, AttachmentError> {
        self.owner_gate(attachment_id, identity, is_admin).await?;
        let mut tx = self.pool.begin().await?;
        let attached =
            AttachmentRepository::attach_to_message(&mut tx, Uuid::new_v4(), message_id, attachment_id)
                .await?;
        stage_bus_event(
            &mut tx,
            "AttachmentAttached",
            "MailMessageAttachment",
            attachment_id,
            crate::domain::event::constants::record_channel("mail.message", message_id),
            "mail.attachment/attach",
            serde_json::json!({
                "attachment_id": attachment_id,
                "message_id": message_id,
            }),
        )
        .await?;
        tx.commit().await?;
        Ok(attached)
    }

    /// Detach an attachment from a message (owner-gated). Returns true when a
    /// join row was removed.
    pub async fn detach_from_message(
        &self,
        identity: &MessagingIdentity,
        is_admin: bool,
        message_id: Uuid,
        attachment_id: Uuid,
    ) -> Result<bool, AttachmentError> {
        self.owner_gate(attachment_id, identity, is_admin).await?;
        let mut tx = self.pool.begin().await?;
        let detached =
            AttachmentRepository::detach_from_message(&mut tx, message_id, attachment_id).await?;
        stage_bus_event(
            &mut tx,
            "AttachmentDetached",
            "MailMessageAttachment",
            attachment_id,
            crate::domain::event::constants::record_channel("mail.message", message_id),
            "mail.attachment/detach",
            serde_json::json!({
                "attachment_id": attachment_id,
                "message_id": message_id,
            }),
        )
        .await?;
        tx.commit().await?;
        Ok(detached)
    }

    /// Mint a fresh public access token (owner-gated). Possession of the
    /// returned uuid IS the read grant (Odoo semantics); clearing is
    /// `set_access_token(None)` on the same gate.
    pub async fn mint_access_token(
        &self,
        identity: &MessagingIdentity,
        is_admin: bool,
        attachment_id: Uuid,
    ) -> Result<Uuid, AttachmentError> {
        self.owner_gate(attachment_id, identity, is_admin).await?;
        let token = Uuid::new_v4();
        let mut tx = self.pool.begin().await?;
        AttachmentRepository::set_access_token(&mut tx, attachment_id, Some(token)).await?;
        stage_bus_event(
            &mut tx,
            "AttachmentTokenMinted",
            "MailAttachment",
            attachment_id,
            identity.channel(),
            "mail.attachment/token_minted",
            // The token VALUE never rides the bus — only the fact of a mint.
            serde_json::json!({ "attachment_id": attachment_id }),
        )
        .await?;
        tx.commit().await?;
        Ok(token)
    }

    /// Clear the public access token (owner-gated) — revokes outstanding
    /// links immediately.
    pub async fn clear_access_token(
        &self,
        identity: &MessagingIdentity,
        is_admin: bool,
        attachment_id: Uuid,
    ) -> Result<(), AttachmentError> {
        self.owner_gate(attachment_id, identity, is_admin).await?;
        let mut tx = self.pool.begin().await?;
        AttachmentRepository::set_access_token(&mut tx, attachment_id, None).await?;
        stage_bus_event(
            &mut tx,
            "AttachmentTokenCleared",
            "MailAttachment",
            attachment_id,
            identity.channel(),
            "mail.attachment/token_cleared",
            serde_json::json!({ "attachment_id": attachment_id }),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// The ownership gate: uploader (partner or guest match) or admin route.
    async fn owner_gate(
        &self,
        attachment_id: Uuid,
        identity: &MessagingIdentity,
        is_admin: bool,
    ) -> Result<(), AttachmentError> {
        if is_admin {
            return Ok(());
        }
        let owners = AttachmentRepository::owners(&self.pool, attachment_id)
            .await?
            .ok_or(AttachmentError::NotFound(attachment_id))?;
        let owns = match identity {
            MessagingIdentity::User { partner_id } => owners.0 == Some(*partner_id),
            MessagingIdentity::Guest { guest_id } => owners.1 == Some(*guest_id),
        };
        if !owns {
            return Err(AttachmentError::Forbidden(attachment_id));
        }
        Ok(())
    }
}
