//! Edge — OTA update management (Phase 14D).

use tracing::{debug, info, warn};

use super::fleet::EdgeFleetManager;
use super::types::{EdgeFleetError, EdgeNodeStatus};

impl EdgeFleetManager {
    /// Mark a node as updating (OTA in progress).
    pub fn start_update(&mut self, node_id: &str) -> Result<(), EdgeFleetError> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| EdgeFleetError::NodeNotFound(node_id.to_string()))?;

        if node.status == EdgeNodeStatus::Decommissioned {
            return Err(EdgeFleetError::NodeDecommissioned(node_id.to_string()));
        }

        if node.active_tasks > 0 {
            return Err(EdgeFleetError::NodeBusy {
                node_id: node_id.to_string(),
                active_tasks: node.active_tasks,
            });
        }

        info!(id = %node_id, name = %node.name, "Edge node update started");
        node.status = EdgeNodeStatus::Updating;
        Ok(())
    }

    /// Mark an update as complete, returning node to online status.
    pub fn complete_update(
        &mut self,
        node_id: &str,
        new_version: String,
    ) -> Result<(), EdgeFleetError> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| EdgeFleetError::NodeNotFound(node_id.to_string()))?;

        if node.status != EdgeNodeStatus::Updating {
            return Err(EdgeFleetError::NotUpdating(node_id.to_string()));
        }

        info!(id = %node_id, name = %node.name, version = %new_version, "Edge node update complete");
        node.agent_version = new_version;
        node.status = EdgeNodeStatus::Online;
        node.last_heartbeat = chrono::Utc::now();
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Phase 14D: Edge Security
    // -----------------------------------------------------------------------

    /// Mark a node as TPM-attested after successful TPM 2.0 attestation.
    ///
    /// In production this would verify a TPM quote against the node's
    /// endorsement key. For now it sets the `tpm_attested` flag to true.
    pub fn attest_node(&mut self, node_id: &str) -> Result<(), EdgeFleetError> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| EdgeFleetError::NodeNotFound(node_id.to_string()))?;

        if node.status == EdgeNodeStatus::Decommissioned {
            return Err(EdgeFleetError::NodeDecommissioned(node_id.to_string()));
        }

        info!(id = %node_id, name = %node.name, "TPM 2.0 attestation passed");
        node.tpm_attested = true;
        Ok(())
    }

    /// Check whether a node has passed TPM attestation.
    pub fn require_attestation(&self, node_id: &str) -> Result<bool, EdgeFleetError> {
        let node = self
            .nodes
            .get(node_id)
            .ok_or_else(|| EdgeFleetError::NodeNotFound(node_id.to_string()))?;
        Ok(node.tpm_attested)
    }

    /// Verify the signature of an OTA update for a given node.
    ///
    /// **STUB**: Currently validates format only (non-empty, valid hex).
    /// Real ed25519 verification is NOT implemented — callers MUST NOT
    /// trust a `true` return in production without replacing this stub.
    pub fn verify_update_signature(
        &self,
        node_id: &str,
        signature: &str,
    ) -> Result<bool, EdgeFleetError> {
        let _node = self
            .nodes
            .get(node_id)
            .ok_or_else(|| EdgeFleetError::NodeNotFound(node_id.to_string()))?;

        if signature.is_empty() {
            debug!(id = %node_id, "OTA signature verification failed: empty signature");
            return Ok(false);
        }

        // Basic format check: signature should be hex-encoded
        if !signature.chars().all(|c| c.is_ascii_hexdigit()) {
            debug!(id = %node_id, "OTA signature verification failed: non-hex characters");
            return Ok(false);
        }

        // SECURITY STUB: This does NOT perform cryptographic verification.
        // TODO: Implement ed25519 verification against update payload hash.
        warn!(id = %node_id, "OTA signature format-checked only (stub — no crypto verification)");
        Ok(true)
    }

    /// Store the update signature on a node (called after a signed OTA is
    /// accepted). Rejects decommissioned nodes.
    pub fn set_update_signature(
        &mut self,
        node_id: &str,
        signature: String,
    ) -> Result<(), EdgeFleetError> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| EdgeFleetError::NodeNotFound(node_id.to_string()))?;

        if node.status == EdgeNodeStatus::Decommissioned {
            return Err(EdgeFleetError::NodeDecommissioned(node_id.to_string()));
        }

        node.update_signature = Some(signature);
        Ok(())
    }

    /// Set the SHA-256 hash of the parent node's TLS certificate.
    ///
    /// Edge nodes pin this hash so they only trust their registered parent.
    /// The hash must be exactly 64 hex characters (SHA-256 digest).
    pub fn set_parent_cert_pin(&mut self, pin_hash: String) -> Result<(), EdgeFleetError> {
        if pin_hash.len() != 64 || !pin_hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(EdgeFleetError::InvalidName(
                "cert pin must be 64 hex characters (SHA-256)".to_string(),
            ));
        }
        info!(hash = %pin_hash, "Parent certificate pin set");
        self.parent_cert_pin = Some(pin_hash);
        Ok(())
    }

    /// Verify that a given certificate hash matches the pinned parent cert.
    ///
    /// Returns `true` if the hash matches, `false` if it does not match or
    /// no pin has been set.
    pub fn verify_parent_cert(&self, cert_hash: &str) -> bool {
        match &self.parent_cert_pin {
            Some(pin) => pin == cert_hash,
            None => false,
        }
    }
}
