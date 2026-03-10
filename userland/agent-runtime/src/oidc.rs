//! Unified SSO/OIDC Provider
//!
//! AGNOS as an OIDC-aware service: issues and validates JWT tokens for agents,
//! users, and external services. Supports standard OIDC discovery, token
//! introspection, and integration with external identity providers.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// OIDC provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcConfig {
    /// Whether OIDC authentication is enabled.
    pub enabled: bool,
    /// Issuer URL (e.g., `https://auth.agnos.local`).
    pub issuer: String,
    /// JWKS endpoint for key discovery.
    pub jwks_uri: String,
    /// Token endpoint for token exchange.
    pub token_endpoint: String,
    /// Authorization endpoint for interactive login.
    pub authorization_endpoint: String,
    /// Userinfo endpoint.
    pub userinfo_endpoint: String,
    /// Supported scopes.
    pub scopes_supported: Vec<String>,
    /// Token lifetime in seconds.
    pub token_lifetime_secs: u64,
    /// Refresh token lifetime in seconds.
    pub refresh_token_lifetime_secs: u64,
    /// Allowed client IDs.
    pub allowed_clients: Vec<String>,
    /// External identity providers for federation.
    pub external_providers: Vec<ExternalIdProvider>,
}

impl Default for OidcConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            issuer: "https://auth.agnos.local".to_string(),
            jwks_uri: "https://auth.agnos.local/.well-known/jwks.json".to_string(),
            token_endpoint: "https://auth.agnos.local/oauth2/token".to_string(),
            authorization_endpoint: "https://auth.agnos.local/oauth2/authorize".to_string(),
            userinfo_endpoint: "https://auth.agnos.local/oauth2/userinfo".to_string(),
            scopes_supported: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
                "agents:read".to_string(),
                "agents:write".to_string(),
                "marketplace:read".to_string(),
                "marketplace:publish".to_string(),
                "vectors:read".to_string(),
                "vectors:write".to_string(),
            ],
            token_lifetime_secs: 3600,
            refresh_token_lifetime_secs: 86400,
            allowed_clients: vec![],
            external_providers: vec![],
        }
    }
}

impl OidcConfig {
    /// Parse from TOML string.
    pub fn from_toml(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }

    /// Generate the OIDC discovery document (`.well-known/openid-configuration`).
    pub fn discovery_document(&self) -> OidcDiscovery {
        OidcDiscovery {
            issuer: self.issuer.clone(),
            authorization_endpoint: self.authorization_endpoint.clone(),
            token_endpoint: self.token_endpoint.clone(),
            userinfo_endpoint: self.userinfo_endpoint.clone(),
            jwks_uri: self.jwks_uri.clone(),
            scopes_supported: self.scopes_supported.clone(),
            response_types_supported: vec![
                "code".to_string(),
                "token".to_string(),
                "id_token".to_string(),
            ],
            grant_types_supported: vec![
                "authorization_code".to_string(),
                "client_credentials".to_string(),
                "refresh_token".to_string(),
            ],
            subject_types_supported: vec!["public".to_string()],
            id_token_signing_alg_values_supported: vec!["RS256".to_string(), "EdDSA".to_string()],
            token_endpoint_auth_methods_supported: vec![
                "client_secret_basic".to_string(),
                "client_secret_post".to_string(),
            ],
        }
    }
}

/// External identity provider for federated SSO.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalIdProvider {
    /// Provider identifier (e.g., "github", "google", "corporate-ad").
    pub id: String,
    /// Display name.
    pub display_name: String,
    /// Provider type.
    pub provider_type: IdProviderType,
    /// OIDC discovery URL.
    pub discovery_url: String,
    /// Client ID for this provider.
    pub client_id: String,
    /// Scopes to request from external provider.
    pub scopes: Vec<String>,
    /// Claim mapping: external claim name → AGNOS claim name.
    pub claim_mapping: HashMap<String, String>,
}

/// Supported external identity provider types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdProviderType {
    /// Standard OIDC provider.
    Oidc,
    /// SAML 2.0 provider (enterprise).
    Saml,
    /// LDAP/Active Directory.
    Ldap,
}

// ---------------------------------------------------------------------------
// OIDC Discovery Document
// ---------------------------------------------------------------------------

/// Standard OIDC discovery document per RFC 8414.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcDiscovery {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: String,
    pub jwks_uri: String,
    pub scopes_supported: Vec<String>,
    pub response_types_supported: Vec<String>,
    pub grant_types_supported: Vec<String>,
    pub subject_types_supported: Vec<String>,
    pub id_token_signing_alg_values_supported: Vec<String>,
    pub token_endpoint_auth_methods_supported: Vec<String>,
}

// ---------------------------------------------------------------------------
// JWT Claims
// ---------------------------------------------------------------------------

/// Standard + AGNOS-specific JWT claims.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgnosClaims {
    /// Issuer.
    pub iss: String,
    /// Subject (user ID or agent ID).
    pub sub: String,
    /// Audience.
    pub aud: String,
    /// Expiration (Unix timestamp).
    pub exp: u64,
    /// Issued at (Unix timestamp).
    pub iat: u64,
    /// Not before (Unix timestamp).
    pub nbf: u64,
    /// JWT ID.
    pub jti: String,
    /// Granted scopes.
    pub scope: String,

    // --- AGNOS-specific claims ---
    /// Subject type: "user", "agent", "service".
    #[serde(default)]
    pub sub_type: String,
    /// Agent ID (when sub_type = "agent").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Publisher key ID (when sub_type = "service" and marketplace operations).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher_key_id: Option<String>,
    /// Allowed operations (fine-grained beyond scopes).
    #[serde(default)]
    pub operations: Vec<String>,
}

impl AgnosClaims {
    /// Check if claims grant a specific scope.
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scope.split_whitespace().any(|s| s == scope)
    }

    /// Check if the token has expired.
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now >= self.exp
    }

    /// Check if the token is not yet valid.
    pub fn is_not_yet_valid(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now < self.nbf
    }

    /// Validate all temporal claims.
    pub fn validate_temporal(&self) -> Result<(), TokenError> {
        if self.is_expired() {
            return Err(TokenError::Expired);
        }
        if self.is_not_yet_valid() {
            return Err(TokenError::NotYetValid);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Token types
// ---------------------------------------------------------------------------

/// Token response from the token endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
    pub scope: String,
}

/// Token introspection response per RFC 7662.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenIntrospection {
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iat: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aud: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,
}

impl TokenIntrospection {
    /// Create an inactive introspection response.
    pub fn inactive() -> Self {
        Self {
            active: false,
            scope: None,
            client_id: None,
            username: None,
            token_type: None,
            exp: None,
            iat: None,
            sub: None,
            aud: None,
            iss: None,
        }
    }

    /// Create from validated claims.
    pub fn from_claims(claims: &AgnosClaims, client_id: &str) -> Self {
        Self {
            active: !claims.is_expired(),
            scope: Some(claims.scope.clone()),
            client_id: Some(client_id.to_string()),
            username: Some(claims.sub.clone()),
            token_type: Some("Bearer".to_string()),
            exp: Some(claims.exp),
            iat: Some(claims.iat),
            sub: Some(claims.sub.clone()),
            aud: Some(claims.aud.clone()),
            iss: Some(claims.iss.clone()),
        }
    }
}

/// Token grant request types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "grant_type")]
pub enum TokenGrant {
    /// Client credentials grant (service-to-service).
    #[serde(rename = "client_credentials")]
    ClientCredentials {
        client_id: String,
        client_secret: String,
        scope: Option<String>,
    },
    /// Authorization code grant (user login).
    #[serde(rename = "authorization_code")]
    AuthorizationCode {
        code: String,
        redirect_uri: String,
        client_id: String,
    },
    /// Refresh token grant.
    #[serde(rename = "refresh_token")]
    RefreshToken {
        refresh_token: String,
        client_id: String,
    },
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Token validation errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenError {
    /// Token has expired.
    Expired,
    /// Token not yet valid (nbf > now).
    NotYetValid,
    /// Invalid signature.
    InvalidSignature,
    /// Missing required scope.
    InsufficientScope(String),
    /// Unknown issuer.
    UnknownIssuer,
    /// Malformed token.
    Malformed(String),
    /// Client not allowed.
    ClientNotAllowed,
    /// Revoked token.
    Revoked,
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Expired => write!(f, "token expired"),
            Self::NotYetValid => write!(f, "token not yet valid"),
            Self::InvalidSignature => write!(f, "invalid token signature"),
            Self::InsufficientScope(s) => write!(f, "missing required scope: {}", s),
            Self::UnknownIssuer => write!(f, "unknown token issuer"),
            Self::Malformed(msg) => write!(f, "malformed token: {}", msg),
            Self::ClientNotAllowed => write!(f, "client not allowed"),
            Self::Revoked => write!(f, "token has been revoked"),
        }
    }
}

impl std::error::Error for TokenError {}

// ---------------------------------------------------------------------------
// Token Provider
// ---------------------------------------------------------------------------

/// OIDC token provider — issues and validates tokens.
#[derive(Debug, Clone)]
pub struct OidcProvider {
    config: OidcConfig,
    /// Registered client credentials: client_id → client_secret.
    clients: HashMap<String, ClientRegistration>,
    /// Revoked token JTIs.
    revoked_tokens: Vec<String>,
}

/// Registered OAuth2 client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRegistration {
    pub client_id: String,
    pub client_secret_hash: String,
    pub display_name: String,
    pub allowed_scopes: Vec<String>,
    pub redirect_uris: Vec<String>,
    /// Client type: "confidential" or "public".
    pub client_type: String,
}

impl OidcProvider {
    /// Create a new OIDC provider with the given configuration.
    pub fn new(config: OidcConfig) -> Self {
        info!(issuer = %config.issuer, "OIDC provider initialised");
        Self {
            config,
            clients: HashMap::new(),
            revoked_tokens: Vec::new(),
        }
    }

    /// Get the OIDC configuration.
    pub fn config(&self) -> &OidcConfig {
        &self.config
    }

    /// Register an OAuth2 client.
    pub fn register_client(&mut self, registration: ClientRegistration) {
        info!(
            client_id = %registration.client_id,
            name = %registration.display_name,
            "Registered OAuth2 client"
        );
        self.clients
            .insert(registration.client_id.clone(), registration);
    }

    /// Look up a client by ID.
    pub fn get_client(&self, client_id: &str) -> Option<&ClientRegistration> {
        self.clients.get(client_id)
    }

    /// List registered client IDs.
    pub fn client_ids(&self) -> Vec<String> {
        let mut ids: Vec<_> = self.clients.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// Issue a token for service-to-service (client_credentials) auth.
    pub fn issue_client_credentials_token(
        &self,
        client_id: &str,
        scope: &str,
    ) -> Result<TokenResponse, TokenError> {
        let client = self
            .clients
            .get(client_id)
            .ok_or(TokenError::ClientNotAllowed)?;

        // Validate requested scopes against allowed scopes.
        let requested_scopes: Vec<&str> = scope.split_whitespace().collect();
        for s in &requested_scopes {
            if !client.allowed_scopes.iter().any(|a| a == s) {
                return Err(TokenError::InsufficientScope(s.to_string()));
            }
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let jti = format!("cc-{}-{}", client_id, now);

        let claims = AgnosClaims {
            iss: self.config.issuer.clone(),
            sub: client_id.to_string(),
            aud: self.config.issuer.clone(),
            exp: now + self.config.token_lifetime_secs,
            iat: now,
            nbf: now,
            jti: jti.clone(),
            scope: scope.to_string(),
            sub_type: "service".to_string(),
            agent_id: None,
            publisher_key_id: None,
            operations: vec![],
        };

        debug!(client_id, scope, "Issued client credentials token");

        Ok(TokenResponse {
            access_token: serde_json::to_string(&claims).unwrap_or_default(),
            token_type: "Bearer".to_string(),
            expires_in: self.config.token_lifetime_secs,
            refresh_token: None,
            id_token: None,
            scope: scope.to_string(),
        })
    }

    /// Issue a token for an agent.
    pub fn issue_agent_token(
        &self,
        agent_id: &str,
        agent_name: &str,
        scopes: &[&str],
    ) -> TokenResponse {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let scope = scopes.join(" ");
        let jti = format!("agent-{}-{}", agent_id, now);

        let claims = AgnosClaims {
            iss: self.config.issuer.clone(),
            sub: agent_name.to_string(),
            aud: self.config.issuer.clone(),
            exp: now + self.config.token_lifetime_secs,
            iat: now,
            nbf: now,
            jti,
            scope: scope.clone(),
            sub_type: "agent".to_string(),
            agent_id: Some(agent_id.to_string()),
            publisher_key_id: None,
            operations: vec![],
        };

        info!(agent_id, agent_name, "Issued agent token");

        TokenResponse {
            access_token: serde_json::to_string(&claims).unwrap_or_default(),
            token_type: "Bearer".to_string(),
            expires_in: self.config.token_lifetime_secs,
            refresh_token: None,
            id_token: None,
            scope,
        }
    }

    /// Validate and decode a token's claims.
    pub fn validate_token(&self, token: &str) -> Result<AgnosClaims, TokenError> {
        let claims: AgnosClaims = serde_json::from_str(token)
            .map_err(|e| TokenError::Malformed(e.to_string()))?;

        // Check issuer.
        if claims.iss != self.config.issuer {
            return Err(TokenError::UnknownIssuer);
        }

        // Check temporal validity.
        claims.validate_temporal()?;

        // Check revocation.
        if self.revoked_tokens.contains(&claims.jti) {
            return Err(TokenError::Revoked);
        }

        Ok(claims)
    }

    /// Validate a token and check for a required scope.
    pub fn validate_token_with_scope(
        &self,
        token: &str,
        required_scope: &str,
    ) -> Result<AgnosClaims, TokenError> {
        let claims = self.validate_token(token)?;
        if !claims.has_scope(required_scope) {
            return Err(TokenError::InsufficientScope(required_scope.to_string()));
        }
        Ok(claims)
    }

    /// Introspect a token (RFC 7662).
    pub fn introspect(&self, token: &str, client_id: &str) -> TokenIntrospection {
        match self.validate_token(token) {
            Ok(claims) => TokenIntrospection::from_claims(&claims, client_id),
            Err(_) => TokenIntrospection::inactive(),
        }
    }

    /// Revoke a token by its JTI.
    pub fn revoke_token(&mut self, jti: &str) {
        if !self.revoked_tokens.contains(&jti.to_string()) {
            self.revoked_tokens.push(jti.to_string());
            warn!(jti, "Token revoked");
        }
    }

    /// Get OIDC discovery document.
    pub fn discovery(&self) -> OidcDiscovery {
        self.config.discovery_document()
    }

    /// Number of registered clients.
    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    /// Number of revoked tokens.
    pub fn revoked_count(&self) -> usize {
        self.revoked_tokens.len()
    }
}

// ---------------------------------------------------------------------------
// Scope constants
// ---------------------------------------------------------------------------

/// Well-known AGNOS scopes.
pub mod scopes {
    pub const OPENID: &str = "openid";
    pub const PROFILE: &str = "profile";
    pub const EMAIL: &str = "email";
    pub const AGENTS_READ: &str = "agents:read";
    pub const AGENTS_WRITE: &str = "agents:write";
    pub const MARKETPLACE_READ: &str = "marketplace:read";
    pub const MARKETPLACE_PUBLISH: &str = "marketplace:publish";
    pub const VECTORS_READ: &str = "vectors:read";
    pub const VECTORS_WRITE: &str = "vectors:write";
    pub const ADMIN: &str = "admin";
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> OidcConfig {
        OidcConfig {
            enabled: true,
            ..Default::default()
        }
    }

    fn test_provider() -> OidcProvider {
        let mut provider = OidcProvider::new(test_config());
        provider.register_client(ClientRegistration {
            client_id: "daimon".to_string(),
            client_secret_hash: "hashed-secret".to_string(),
            display_name: "Daimon Agent Runtime".to_string(),
            allowed_scopes: vec![
                "openid".to_string(),
                "agents:read".to_string(),
                "agents:write".to_string(),
                "marketplace:read".to_string(),
            ],
            redirect_uris: vec![],
            client_type: "confidential".to_string(),
        });
        provider
    }

    #[test]
    fn test_oidc_config_default() {
        let config = OidcConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.issuer, "https://auth.agnos.local");
        assert!(config.scopes_supported.contains(&"openid".to_string()));
        assert!(config.scopes_supported.contains(&"agents:read".to_string()));
        assert_eq!(config.token_lifetime_secs, 3600);
    }

    #[test]
    fn test_oidc_config_from_toml() {
        let toml = r#"
enabled = true
issuer = "https://my-issuer.example.com"
jwks_uri = "https://my-issuer.example.com/.well-known/jwks.json"
token_endpoint = "https://my-issuer.example.com/token"
authorization_endpoint = "https://my-issuer.example.com/authorize"
userinfo_endpoint = "https://my-issuer.example.com/userinfo"
scopes_supported = ["openid", "agents:read"]
token_lifetime_secs = 1800
refresh_token_lifetime_secs = 43200
allowed_clients = ["my-app"]
external_providers = []
"#;
        let config = OidcConfig::from_toml(toml).unwrap();
        assert!(config.enabled);
        assert_eq!(config.issuer, "https://my-issuer.example.com");
        assert_eq!(config.token_lifetime_secs, 1800);
    }

    #[test]
    fn test_discovery_document() {
        let config = test_config();
        let doc = config.discovery_document();
        assert_eq!(doc.issuer, config.issuer);
        assert_eq!(doc.token_endpoint, config.token_endpoint);
        assert!(doc.response_types_supported.contains(&"code".to_string()));
        assert!(doc
            .grant_types_supported
            .contains(&"client_credentials".to_string()));
        assert!(doc
            .id_token_signing_alg_values_supported
            .contains(&"EdDSA".to_string()));
    }

    #[test]
    fn test_register_client() {
        let provider = test_provider();
        assert_eq!(provider.client_count(), 1);
        assert!(provider.get_client("daimon").is_some());
        assert!(provider.get_client("unknown").is_none());
        assert_eq!(provider.client_ids(), vec!["daimon"]);
    }

    #[test]
    fn test_issue_client_credentials_token() {
        let provider = test_provider();
        let response = provider
            .issue_client_credentials_token("daimon", "agents:read agents:write")
            .unwrap();
        assert_eq!(response.token_type, "Bearer");
        assert_eq!(response.expires_in, 3600);
        assert_eq!(response.scope, "agents:read agents:write");
        assert!(response.refresh_token.is_none());
    }

    #[test]
    fn test_issue_client_credentials_unknown_client() {
        let provider = test_provider();
        let result = provider.issue_client_credentials_token("unknown", "agents:read");
        assert_eq!(result.unwrap_err(), TokenError::ClientNotAllowed);
    }

    #[test]
    fn test_issue_client_credentials_insufficient_scope() {
        let provider = test_provider();
        let result = provider.issue_client_credentials_token("daimon", "admin");
        assert!(matches!(
            result.unwrap_err(),
            TokenError::InsufficientScope(_)
        ));
    }

    #[test]
    fn test_issue_agent_token() {
        let provider = test_provider();
        let response = provider.issue_agent_token("agent-123", "my-agent", &["agents:read"]);
        assert_eq!(response.token_type, "Bearer");
        assert_eq!(response.scope, "agents:read");

        // Validate the issued token
        let claims = provider.validate_token(&response.access_token).unwrap();
        assert_eq!(claims.sub, "my-agent");
        assert_eq!(claims.sub_type, "agent");
        assert_eq!(claims.agent_id, Some("agent-123".to_string()));
    }

    #[test]
    fn test_validate_token_success() {
        let provider = test_provider();
        let response = provider
            .issue_client_credentials_token("daimon", "agents:read")
            .unwrap();
        let claims = provider.validate_token(&response.access_token).unwrap();
        assert_eq!(claims.iss, "https://auth.agnos.local");
        assert_eq!(claims.sub, "daimon");
        assert!(claims.has_scope("agents:read"));
    }

    #[test]
    fn test_validate_token_wrong_issuer() {
        let provider = test_provider();
        let claims = AgnosClaims {
            iss: "https://evil.example.com".to_string(),
            sub: "attacker".to_string(),
            aud: "x".to_string(),
            exp: u64::MAX,
            iat: 0,
            nbf: 0,
            jti: "bad".to_string(),
            scope: "admin".to_string(),
            sub_type: "user".to_string(),
            agent_id: None,
            publisher_key_id: None,
            operations: vec![],
        };
        let token = serde_json::to_string(&claims).unwrap();
        assert_eq!(
            provider.validate_token(&token).unwrap_err(),
            TokenError::UnknownIssuer
        );
    }

    #[test]
    fn test_validate_token_expired() {
        let provider = test_provider();
        let claims = AgnosClaims {
            iss: "https://auth.agnos.local".to_string(),
            sub: "test".to_string(),
            aud: "x".to_string(),
            exp: 1, // long expired
            iat: 0,
            nbf: 0,
            jti: "expired".to_string(),
            scope: "openid".to_string(),
            sub_type: "user".to_string(),
            agent_id: None,
            publisher_key_id: None,
            operations: vec![],
        };
        let token = serde_json::to_string(&claims).unwrap();
        assert_eq!(
            provider.validate_token(&token).unwrap_err(),
            TokenError::Expired
        );
    }

    #[test]
    fn test_validate_token_malformed() {
        let provider = test_provider();
        assert!(matches!(
            provider.validate_token("not-json").unwrap_err(),
            TokenError::Malformed(_)
        ));
    }

    #[test]
    fn test_validate_token_with_scope() {
        let provider = test_provider();
        let response = provider
            .issue_client_credentials_token("daimon", "agents:read marketplace:read")
            .unwrap();
        assert!(provider
            .validate_token_with_scope(&response.access_token, "agents:read")
            .is_ok());
        assert!(matches!(
            provider
                .validate_token_with_scope(&response.access_token, "admin")
                .unwrap_err(),
            TokenError::InsufficientScope(_)
        ));
    }

    #[test]
    fn test_revoke_token() {
        let mut provider = test_provider();
        let response = provider
            .issue_client_credentials_token("daimon", "agents:read")
            .unwrap();
        let claims = provider.validate_token(&response.access_token).unwrap();

        provider.revoke_token(&claims.jti);
        assert_eq!(provider.revoked_count(), 1);
        assert_eq!(
            provider.validate_token(&response.access_token).unwrap_err(),
            TokenError::Revoked
        );
    }

    #[test]
    fn test_introspect_active() {
        let provider = test_provider();
        let response = provider
            .issue_client_credentials_token("daimon", "agents:read")
            .unwrap();
        let introspection = provider.introspect(&response.access_token, "daimon");
        assert!(introspection.active);
        assert_eq!(introspection.scope, Some("agents:read".to_string()));
        assert_eq!(introspection.client_id, Some("daimon".to_string()));
    }

    #[test]
    fn test_introspect_invalid() {
        let provider = test_provider();
        let introspection = provider.introspect("garbage-token", "daimon");
        assert!(!introspection.active);
    }

    #[test]
    fn test_claims_has_scope() {
        let claims = AgnosClaims {
            iss: "x".to_string(),
            sub: "x".to_string(),
            aud: "x".to_string(),
            exp: u64::MAX,
            iat: 0,
            nbf: 0,
            jti: "x".to_string(),
            scope: "openid agents:read marketplace:publish".to_string(),
            sub_type: "user".to_string(),
            agent_id: None,
            publisher_key_id: None,
            operations: vec![],
        };
        assert!(claims.has_scope("openid"));
        assert!(claims.has_scope("agents:read"));
        assert!(claims.has_scope("marketplace:publish"));
        assert!(!claims.has_scope("admin"));
        assert!(!claims.has_scope("agents:write"));
    }

    #[test]
    fn test_token_error_display() {
        assert_eq!(TokenError::Expired.to_string(), "token expired");
        assert_eq!(
            TokenError::InsufficientScope("admin".to_string()).to_string(),
            "missing required scope: admin"
        );
        assert_eq!(TokenError::Revoked.to_string(), "token has been revoked");
    }

    #[test]
    fn test_token_response_serialization() {
        let resp = TokenResponse {
            access_token: "abc".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: 3600,
            refresh_token: Some("refresh-xyz".to_string()),
            id_token: None,
            scope: "openid".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: TokenResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.access_token, "abc");
        assert_eq!(parsed.refresh_token, Some("refresh-xyz".to_string()));
        assert!(parsed.id_token.is_none());
    }

    #[test]
    fn test_external_id_provider_serialization() {
        let provider = ExternalIdProvider {
            id: "github".to_string(),
            display_name: "GitHub".to_string(),
            provider_type: IdProviderType::Oidc,
            discovery_url: "https://token.actions.githubusercontent.com".to_string(),
            client_id: "gh-client".to_string(),
            scopes: vec!["openid".to_string()],
            claim_mapping: HashMap::from([("login".to_string(), "username".to_string())]),
        };
        let json = serde_json::to_string(&provider).unwrap();
        let parsed: ExternalIdProvider = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "github");
        assert_eq!(parsed.provider_type, IdProviderType::Oidc);
        assert_eq!(parsed.claim_mapping.get("login"), Some(&"username".to_string()));
    }

    #[test]
    fn test_token_grant_variants() {
        let cc = serde_json::json!({
            "grant_type": "client_credentials",
            "client_id": "daimon",
            "client_secret": "secret",
            "scope": "agents:read"
        });
        let grant: TokenGrant = serde_json::from_value(cc).unwrap();
        assert!(matches!(grant, TokenGrant::ClientCredentials { .. }));

        let refresh = serde_json::json!({
            "grant_type": "refresh_token",
            "refresh_token": "rt-abc",
            "client_id": "daimon"
        });
        let grant: TokenGrant = serde_json::from_value(refresh).unwrap();
        assert!(matches!(grant, TokenGrant::RefreshToken { .. }));
    }

    #[test]
    fn test_inactive_introspection() {
        let intr = TokenIntrospection::inactive();
        assert!(!intr.active);
        assert!(intr.scope.is_none());
        assert!(intr.sub.is_none());
    }

    #[test]
    fn test_id_provider_types() {
        let json = serde_json::to_string(&IdProviderType::Saml).unwrap();
        assert_eq!(json, "\"Saml\"");
        let parsed: IdProviderType = serde_json::from_str("\"Ldap\"").unwrap();
        assert_eq!(parsed, IdProviderType::Ldap);
    }
}
