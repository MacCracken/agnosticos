//! Inter-Process Communication for Agents
//!
//! Handles message passing between agents and the runtime.
//! Uses length-prefixed framing: each message is preceded by a 4-byte big-endian length.

use std::collections::HashMap;

use anyhow::{Context, Result};
use tokio::io::AsyncReadExt;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use agnos_common::{AgentId, Message, MessageType};

/// Maximum size of a single IPC message (64 KB).
const MAX_MESSAGE_SIZE: u32 = 64 * 1024;

/// Maximum number of global (monitoring) subscribers.
const MAX_GLOBAL_SUBSCRIBERS: usize = 16;

/// IPC endpoint for agent communication
pub struct AgentIpc {
    agent_id: AgentId,
    socket_path: std::path::PathBuf,
    message_tx: mpsc::Sender<Message>,
    message_rx: Option<mpsc::Receiver<Message>>,
}

impl AgentIpc {
    /// Create a new IPC endpoint for an agent
    pub fn new(agent_id: AgentId) -> Result<(Self, mpsc::Receiver<Message>)> {
        let socket_path = std::path::PathBuf::from(format!("/run/agnos/agents/{}.sock", agent_id));
        let (message_tx, message_rx) = mpsc::channel(100);
        
        let ipc = Self {
            agent_id,
            socket_path,
            message_tx,
            message_rx: None,
        };

        Ok((ipc, message_rx))
    }

    /// Start listening for incoming connections.
    ///
    /// Sets restrictive permissions (owner-only) on the socket file
    /// to prevent unauthorized access from other users.
    pub async fn start_listening(&self) -> Result<()> {
        // Ensure directory exists
        if let Some(parent) = self.socket_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Remove old socket if it exists
        let _ = tokio::fs::remove_file(&self.socket_path).await;

        let listener = UnixListener::bind(&self.socket_path)
            .with_context(|| format!("Failed to bind to socket: {}", self.socket_path.display()))?;

        // Restrict socket to owner only (0o700)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o700);
            std::fs::set_permissions(&self.socket_path, perms)
                .with_context(|| "Failed to set socket permissions")?;
        }

        info!("Agent {} IPC listening on {}", self.agent_id, self.socket_path.display());

        let tx = self.message_tx.clone();
        let agent_id = self.agent_id;

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let tx = tx.clone();
                        tokio::spawn(handle_connection(stream, tx, agent_id));
                    }
                    Err(e) => {
                        error!("Failed to accept connection: {}", e);
                    }
                }
            }
        });

        Ok(())
    }

    /// Send a message to this agent
    pub async fn send(&self, message: Message) -> Result<()> {
        self.message_tx.send(message).await
            .map_err(|_| anyhow::anyhow!("Failed to send message"))?;
        Ok(())
    }
}

impl Drop for AgentIpc {
    fn drop(&mut self) {
        // Clean up socket file on shutdown
        let _ = std::fs::remove_file(&self.socket_path);
        debug!("Cleaned up IPC socket: {}", self.socket_path.display());
    }
}

/// Handle an incoming connection using length-prefixed framing.
///
/// Wire format: `[4-byte big-endian length][JSON message bytes]`
/// Messages larger than `MAX_MESSAGE_SIZE` are rejected and the connection is closed.
async fn handle_connection(mut stream: UnixStream, tx: mpsc::Sender<Message>, owner_agent_id: AgentId) {
    let mut len_buf = [0u8; 4];

    loop {
        // Read the 4-byte length prefix
        match stream.read_exact(&mut len_buf).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                // Connection closed cleanly
                break;
            }
            Err(e) => {
                error!("Failed to read message length: {}", e);
                break;
            }
        }

        let msg_len = u32::from_be_bytes(len_buf);

        if msg_len == 0 {
            continue;
        }

        if msg_len > MAX_MESSAGE_SIZE {
            error!(
                "Message too large ({} bytes, max {}), closing connection",
                msg_len, MAX_MESSAGE_SIZE
            );
            break;
        }

        let mut buffer = vec![0u8; msg_len as usize];
        if let Err(e) = stream.read_exact(&mut buffer).await {
            error!("Failed to read message body: {}", e);
            break;
        }

        // Parse message
        match serde_json::from_slice::<Message>(&buffer) {
            Ok(message) => {
                debug!("Received message: {:?}", message);
                if let Err(e) = tx.send(message).await {
                    error!("Failed to forward message: {}", e);
                }
            }
            Err(e) => {
                warn!("Failed to parse message: {}", e);
            }
        }
    }
}

/// IPC bus for routing messages between agents
pub struct MessageBus {
    /// Subscribers by agent ID
    subscribers: RwLock<HashMap<AgentId, mpsc::Sender<Message>>>,
    /// Agent name to ID mapping for routing
    agent_names: RwLock<HashMap<String, AgentId>>,
    /// Global subscribers (receive all messages)
    global_subscribers: RwLock<Vec<mpsc::Sender<Message>>>,
}

impl MessageBus {
    /// Create a new message bus
    pub fn new() -> Self {
        Self {
            subscribers: RwLock::new(HashMap::new()),
            agent_names: RwLock::new(HashMap::new()),
            global_subscribers: RwLock::new(Vec::new()),
        }
    }

    /// Subscribe an agent to receive messages
    pub async fn subscribe(&self, agent_id: AgentId, sender: mpsc::Sender<Message>) {
        self.subscribers.write().await.insert(agent_id, sender);
    }
    
    /// Register an agent by name for routing
    pub async fn register_agent_name(&self, agent_id: AgentId, name: &str) {
        self.agent_names.write().await.insert(name.to_string(), agent_id);
    }
    
    /// Unregister an agent name
    pub async fn unregister_agent_name(&self, name: &str) {
        self.agent_names.write().await.remove(name);
    }
    
    /// Get agent ID by name
    pub async fn get_agent_id(&self, name: &str) -> Option<AgentId> {
        self.agent_names.read().await.get(name).cloned()
    }

    /// Unsubscribe an agent
    pub async fn unsubscribe(&self, agent_id: AgentId) {
        self.subscribers.write().await.remove(&agent_id);
    }

    /// Subscribe to all messages (for monitoring/debugging).
    ///
    /// Limited to [`MAX_GLOBAL_SUBSCRIBERS`] to prevent unbounded growth.
    pub async fn subscribe_global(&self, sender: mpsc::Sender<Message>) -> Result<()> {
        let mut globals = self.global_subscribers.write().await;
        if globals.len() >= MAX_GLOBAL_SUBSCRIBERS {
            return Err(anyhow::anyhow!(
                "Maximum global subscribers ({}) reached",
                MAX_GLOBAL_SUBSCRIBERS
            ));
        }
        globals.push(sender);
        Ok(())
    }

    /// Publish a message
    pub async fn publish(&self, message: Message) -> Result<()> {
        // Check if message has a specific target
        if message.target != "*" && message.target != "broadcast" {
            // Try to route to specific agent by name
            let agent_id = {
                let names = self.agent_names.read().await;
                names.get(&message.target).cloned()
            };
            
            if let Some(target_id) = agent_id {
                // Send to specific agent
                let subscribers = self.subscribers.read().await;
                if let Some(sender) = subscribers.get(&target_id) {
                    sender.send(message).await
                        .map_err(|_| anyhow::anyhow!("Failed to send message to agent"))?;
                }
                return Ok(());
            }
            
            // Agent not found - could broadcast instead or return error
            debug!("Message target {} not found, broadcasting", message.target);
        }

        // Send to all subscribers
        let subscribers = self.subscribers.read().await;
        for (agent_id, sender) in subscribers.iter() {
            if sender.send(message.clone()).await.is_err() {
                warn!("Failed to send message to agent {}", agent_id);
            }
        }

        // Send to global subscribers
        let globals = self.global_subscribers.read().await;
        for sender in globals.iter() {
            let _ = sender.send(message.clone()).await;
        }

        Ok(())
    }

    /// Send a message to a specific agent
    pub async fn send_to(&self, agent_id: AgentId, message: Message) -> Result<()> {
        let subscribers = self.subscribers.read().await;
        
        if let Some(sender) = subscribers.get(&agent_id) {
            sender.send(message).await
                .map_err(|_| anyhow::anyhow!("Failed to send message"))?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Agent {} not subscribed", agent_id))
        }
    }
    
    /// Send a message to a specific agent by name
    pub async fn send_to_name(&self, name: &str, message: Message) -> Result<()> {
        let agent_id = {
            let names = self.agent_names.read().await;
            names.get(name).cloned()
        };
        
        match agent_id {
            Some(id) => self.send_to(id, message).await,
            None => Err(anyhow::anyhow!("Agent {} not found", name)),
        }
    }
}

impl Default for MessageBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_ipc_new() {
        let agent_id = AgentId::new();
        let result = AgentIpc::new(agent_id);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_message_bus_new() {
        let bus = MessageBus::new();
        let subscribers = bus.subscribers.read().await.len();
        assert_eq!(subscribers, 0);
    }

    #[tokio::test]
    async fn test_message_bus_subscribe() {
        let bus = MessageBus::new();
        let agent_id = AgentId::new();
        let (tx, _rx) = mpsc::channel(10);
        
        bus.subscribe(agent_id, tx).await;
        
        let subscribers = bus.subscribers.read().await;
        assert!(subscribers.contains_key(&agent_id));
    }

    #[tokio::test]
    async fn test_message_bus_unsubscribe() {
        let bus = MessageBus::new();
        let agent_id = AgentId::new();
        let (tx, _rx) = mpsc::channel(10);
        
        bus.subscribe(agent_id, tx).await;
        bus.unsubscribe(agent_id).await;
        
        let subscribers = bus.subscribers.read().await;
        assert!(!subscribers.contains_key(&agent_id));
    }

    #[tokio::test]
    async fn test_message_bus_subscribe_global() {
        let bus = MessageBus::new();
        let (tx, _rx) = mpsc::channel(10);

        bus.subscribe_global(tx).await.unwrap();

        let globals = bus.global_subscribers.read().await;
        assert_eq!(globals.len(), 1);
    }

    #[tokio::test]
    async fn test_message_bus_subscribe_global_limit() {
        let bus = MessageBus::new();
        for _ in 0..MAX_GLOBAL_SUBSCRIBERS {
            let (tx, _rx) = mpsc::channel(10);
            bus.subscribe_global(tx).await.unwrap();
        }
        // Next should fail
        let (tx, _rx) = mpsc::channel(10);
        assert!(bus.subscribe_global(tx).await.is_err());
    }

    #[tokio::test]
    async fn test_ipc_drop_cleanup() {
        let tmp = std::env::temp_dir().join("agnos_ipc_test");
        let _ = std::fs::create_dir_all(&tmp);
        let sock_path = tmp.join("test.sock");
        // Create a dummy file to simulate a socket
        let _ = std::fs::File::create(&sock_path);
        assert!(sock_path.exists());

        // Build an AgentIpc with the temp path and verify Drop removes it
        let agent_id = AgentId::new();
        let (tx, _rx) = mpsc::channel(10);
        let ipc = AgentIpc {
            agent_id,
            socket_path: sock_path.clone(),
            message_tx: tx,
            message_rx: None,
        };
        drop(ipc);
        assert!(!sock_path.exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn test_message_bus_send_to_existing() {
        let bus = MessageBus::new();
        let agent_id = AgentId::new();
        let (tx, mut rx) = mpsc::channel(10);
        
        bus.subscribe(agent_id, tx).await;
        
        let message = Message {
            id: Uuid::new_v4().to_string(),
            source: "test".to_string(),
            target: agent_id.to_string(),
            message_type: MessageType::Command,
            payload: serde_json::json!({"test": "data"}),
            timestamp: chrono::Utc::now(),
        };
        
        let result = bus.send_to(agent_id, message).await;
        assert!(result.is_ok());
        
        let received = rx.recv().await;
        assert!(received.is_some());
    }

    #[tokio::test]
    async fn test_message_bus_send_to_nonexistent() {
        let bus = MessageBus::new();
        let agent_id = AgentId::new();
        
        let message = Message {
            id: Uuid::new_v4().to_string(),
            source: "test".to_string(),
            target: agent_id.to_string(),
            message_type: MessageType::Command,
            payload: serde_json::json!({}),
            timestamp: chrono::Utc::now(),
        };
        
        let result = bus.send_to(agent_id, message).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_message_bus_publish() {
        let bus = MessageBus::new();
        let agent_id = AgentId::new();
        let (tx, mut rx) = mpsc::channel(10);
        
        bus.subscribe(agent_id, tx).await;
        
        let message = Message {
            id: Uuid::new_v4().to_string(),
            source: "test".to_string(),
            target: "broadcast".to_string(),
            message_type: MessageType::Event,
            payload: serde_json::json!({}),
            timestamp: chrono::Utc::now(),
        };
        
        let result = bus.publish(message).await;
        assert!(result.is_ok());
        
        let received = rx.recv().await;
        assert!(received.is_some());
    }

    #[tokio::test]
    async fn test_message_bus_default() {
        let bus = MessageBus::default();
        let subscribers = bus.subscribers.read().await.len();
        assert_eq!(subscribers, 0);
    }

    #[tokio::test]
    async fn test_message_bus_register_agent_name() {
        let bus = MessageBus::new();
        let agent_id = AgentId::new();
        
        bus.register_agent_name(agent_id, "test-agent").await;
        
        let resolved = bus.get_agent_id("test-agent").await;
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap(), agent_id);
    }
    
    #[tokio::test]
    async fn test_message_bus_unregister_agent_name() {
        let bus = MessageBus::new();
        let agent_id = AgentId::new();
        
        bus.register_agent_name(agent_id, "test-agent").await;
        bus.unregister_agent_name("test-agent").await;
        
        let resolved = bus.get_agent_id("test-agent").await;
        assert!(resolved.is_none());
    }
    
    #[tokio::test]
    async fn test_message_bus_send_to_name() {
        let bus = MessageBus::new();
        let agent_id = AgentId::new();
        let (tx, mut rx) = mpsc::channel(10);
        
        bus.subscribe(agent_id.clone(), tx).await;
        bus.register_agent_name(agent_id, "my-agent").await;
        
        let message = Message {
            id: Uuid::new_v4().to_string(),
            source: "test".to_string(),
            target: "my-agent".to_string(),
            message_type: MessageType::Command,
            payload: serde_json::json!({"test": "data"}),
            timestamp: chrono::Utc::now(),
        };
        
        let result = bus.send_to_name("my-agent", message).await;
        assert!(result.is_ok());
        
        let received = rx.recv().await;
        assert!(received.is_some());
    }
    
    #[tokio::test]
    async fn test_message_bus_send_to_name_not_found() {
        let bus = MessageBus::new();
        
        let message = Message {
            id: Uuid::new_v4().to_string(),
            source: "test".to_string(),
            target: "nonexistent".to_string(),
            message_type: MessageType::Command,
            payload: serde_json::json!({}),
            timestamp: chrono::Utc::now(),
        };
        
        let result = bus.send_to_name("nonexistent", message).await;
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_message_bus_publish_routes_by_name() {
        let bus = MessageBus::new();
        let agent_id = AgentId::new();
        let (tx, mut rx) = mpsc::channel(10);
        
        bus.subscribe(agent_id.clone(), tx).await;
        bus.register_agent_name(agent_id, "target-agent").await;
        
        let message = Message {
            id: Uuid::new_v4().to_string(),
            source: "test".to_string(),
            target: "target-agent".to_string(),
            message_type: MessageType::Command,
            payload: serde_json::json!({"test": "data"}),
            timestamp: chrono::Utc::now(),
        };
        
        let result = bus.publish(message).await;
        assert!(result.is_ok());
        
        let received = rx.recv().await;
        assert!(received.is_some());
    }
}
