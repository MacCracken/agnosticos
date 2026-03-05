//! Inter-Process Communication for Agents
//!
//! Handles message passing between agents and the runtime.
//! Uses length-prefixed framing: each message is preceded by a 4-byte big-endian length.

use std::collections::HashMap;

use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

use agnos_common::{AgentId, Message};
#[cfg(test)]
use agnos_common::MessageType;
#[cfg(test)]
use uuid::Uuid;

/// Maximum size of a single IPC message (64 KB).
const MAX_MESSAGE_SIZE: u32 = 64 * 1024;

/// Maximum number of global (monitoring) subscribers.
const MAX_GLOBAL_SUBSCRIBERS: usize = 16;

/// Maximum concurrent connections per agent socket.
const MAX_CONCURRENT_CONNECTIONS: usize = 64;

/// IPC endpoint for agent communication
pub struct AgentIpc {
    agent_id: AgentId,
    socket_path: std::path::PathBuf,
    message_tx: mpsc::Sender<Message>,
    _message_rx: Option<mpsc::Receiver<Message>>,
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
            _message_rx: None,
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
        let conn_semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_CONNECTIONS));

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let tx = tx.clone();
                        let permit = conn_semaphore.clone().try_acquire_owned();
                        match permit {
                            Ok(permit) => {
                                tokio::spawn(async move {
                                    handle_connection(stream, tx, agent_id).await;
                                    drop(permit);
                                });
                            }
                            Err(_) => {
                                warn!(agent_id = %agent_id, max = MAX_CONCURRENT_CONNECTIONS, "Connection rejected: too many concurrent connections");
                                drop(stream);
                            }
                        }
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

/// Response codes sent back to the client after each message.
const ACK: u8 = 0x01;
const NACK_QUEUE_FULL: u8 = 0x02;
const NACK_INVALID: u8 = 0x03;

/// Handle an incoming connection using length-prefixed framing with backpressure.
///
/// Wire format (request):  `[4-byte big-endian length][JSON message bytes]`
/// Wire format (response): `[1-byte status]` — ACK (0x01), NACK_QUEUE_FULL (0x02), or NACK_INVALID (0x03).
///
/// Messages larger than `MAX_MESSAGE_SIZE` are rejected and the connection is closed.
/// When the agent's message queue is full, the sender receives NACK_QUEUE_FULL and
/// can choose to retry after a delay, providing explicit flow-control signalling.
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
                size = msg_len,
                max = MAX_MESSAGE_SIZE,
                "Message too large, closing connection"
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
                debug!(msg_id = %message.id, agent_id = %owner_agent_id, "Received message");
                // Use try_send for backpressure: if the queue is full, NACK immediately
                match tx.try_send(message) {
                    Ok(()) => {
                        let _ = stream.write_all(&[ACK]).await;
                    }
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        warn!(agent_id = %owner_agent_id, "Message queue full, sending NACK");
                        let _ = stream.write_all(&[NACK_QUEUE_FULL]).await;
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        error!(agent_id = %owner_agent_id, "Message channel closed");
                        break;
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "Failed to parse message");
                let _ = stream.write_all(&[NACK_INVALID]).await;
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
            _message_rx: None,
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

    // ==================================================================
    // Additional coverage: handle_connection, AgentIpc::send, socket
    // path format, MessageBus global subscribers receive broadcasts,
    // publish to wildcard, unsubscribe then send, name overwrite
    // ==================================================================

    #[test]
    fn test_agent_ipc_socket_path_format() {
        let agent_id = AgentId::new();
        let (ipc, _rx) = AgentIpc::new(agent_id).unwrap();
        let expected = format!("/run/agnos/agents/{}.sock", agent_id);
        assert_eq!(ipc.socket_path.to_str().unwrap(), expected);
    }

    #[tokio::test]
    async fn test_agent_ipc_send() {
        let agent_id = AgentId::new();
        let (ipc, mut rx) = AgentIpc::new(agent_id).unwrap();

        let msg = Message {
            id: "ipc-msg-1".to_string(),
            source: "test".to_string(),
            target: agent_id.to_string(),
            message_type: MessageType::Command,
            payload: serde_json::json!({"hello": true}),
            timestamp: chrono::Utc::now(),
        };

        ipc.send(msg).await.unwrap();
        let received = rx.recv().await.unwrap();
        assert_eq!(received.id, "ipc-msg-1");
    }

    #[tokio::test]
    async fn test_message_bus_publish_broadcast_wildcard() {
        let bus = MessageBus::new();
        let agent_id = AgentId::new();
        let (tx, mut rx) = mpsc::channel(10);

        bus.subscribe(agent_id, tx).await;

        // "*" target should broadcast
        let message = Message {
            id: Uuid::new_v4().to_string(),
            source: "orchestrator".to_string(),
            target: "*".to_string(),
            message_type: MessageType::Event,
            payload: serde_json::json!({"event": "shutdown"}),
            timestamp: chrono::Utc::now(),
        };

        bus.publish(message).await.unwrap();
        let received = rx.recv().await;
        assert!(received.is_some());
    }

    #[tokio::test]
    async fn test_message_bus_publish_to_global_subscribers() {
        let bus = MessageBus::new();
        let (global_tx, mut global_rx) = mpsc::channel(10);
        bus.subscribe_global(global_tx).await.unwrap();

        // Broadcast message
        let message = Message {
            id: Uuid::new_v4().to_string(),
            source: "test".to_string(),
            target: "broadcast".to_string(),
            message_type: MessageType::Event,
            payload: serde_json::json!({}),
            timestamp: chrono::Utc::now(),
        };

        bus.publish(message).await.unwrap();
        let received = global_rx.recv().await;
        assert!(received.is_some());
    }

    #[tokio::test]
    async fn test_message_bus_publish_to_multiple_subscribers() {
        let bus = MessageBus::new();

        let id1 = AgentId::new();
        let id2 = AgentId::new();
        let (tx1, mut rx1) = mpsc::channel(10);
        let (tx2, mut rx2) = mpsc::channel(10);

        bus.subscribe(id1, tx1).await;
        bus.subscribe(id2, tx2).await;

        let message = Message {
            id: Uuid::new_v4().to_string(),
            source: "test".to_string(),
            target: "broadcast".to_string(),
            message_type: MessageType::Event,
            payload: serde_json::json!({}),
            timestamp: chrono::Utc::now(),
        };

        bus.publish(message).await.unwrap();

        assert!(rx1.recv().await.is_some());
        assert!(rx2.recv().await.is_some());
    }

    #[tokio::test]
    async fn test_message_bus_send_to_after_unsubscribe() {
        let bus = MessageBus::new();
        let agent_id = AgentId::new();
        let (tx, _rx) = mpsc::channel(10);

        bus.subscribe(agent_id, tx).await;
        bus.unsubscribe(agent_id).await;

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
    async fn test_message_bus_register_name_overwrite() {
        let bus = MessageBus::new();
        let id1 = AgentId::new();
        let id2 = AgentId::new();

        bus.register_agent_name(id1, "shared-name").await;
        bus.register_agent_name(id2, "shared-name").await;

        // Name should now point to id2
        let resolved = bus.get_agent_id("shared-name").await;
        assert_eq!(resolved, Some(id2));
    }

    #[tokio::test]
    async fn test_message_bus_get_agent_id_nonexistent() {
        let bus = MessageBus::new();
        assert!(bus.get_agent_id("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_message_bus_unregister_nonexistent_name() {
        let bus = MessageBus::new();
        // Should not panic
        bus.unregister_agent_name("doesnotexist").await;
    }

    #[tokio::test]
    async fn test_message_bus_publish_unknown_target_broadcasts() {
        let bus = MessageBus::new();
        let agent_id = AgentId::new();
        let (tx, mut rx) = mpsc::channel(10);

        bus.subscribe(agent_id, tx).await;

        // Target name not registered — should broadcast to all
        let message = Message {
            id: Uuid::new_v4().to_string(),
            source: "test".to_string(),
            target: "unknown-agent".to_string(),
            message_type: MessageType::Command,
            payload: serde_json::json!({}),
            timestamp: chrono::Utc::now(),
        };

        bus.publish(message).await.unwrap();
        let received = rx.recv().await;
        assert!(received.is_some());
    }

    #[tokio::test]
    async fn test_handle_connection_with_valid_message() {
        // Test handle_connection via a real Unix socket pair
        let tmp = std::env::temp_dir().join(format!("agnos_ipc_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);
        let sock_path = tmp.join("test_conn.sock");
        let _ = std::fs::remove_file(&sock_path);

        let listener = UnixListener::bind(&sock_path).unwrap();
        let (tx, mut rx) = mpsc::channel(10);
        let agent_id = AgentId::new();

        // Spawn handler
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection(stream, tx, agent_id).await;
        });

        // Connect and send a message with length-prefix framing
        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        let msg = Message {
            id: "conn-test".to_string(),
            source: "client".to_string(),
            target: "server".to_string(),
            message_type: MessageType::Command,
            payload: serde_json::json!({"action": "ping"}),
            timestamp: chrono::Utc::now(),
        };
        let bytes = serde_json::to_vec(&msg).unwrap();
        let len = (bytes.len() as u32).to_be_bytes();

        client.write_all(&len).await.unwrap();
        client.write_all(&bytes).await.unwrap();

        // Read the ACK response
        let mut ack = [0u8; 1];
        client.read_exact(&mut ack).await.unwrap();
        assert_eq!(ack[0], ACK);

        drop(client); // Close connection

        // Should receive the message
        let received = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            rx.recv(),
        ).await.unwrap();
        assert!(received.is_some());
        assert_eq!(received.unwrap().id, "conn-test");

        let _ = std::fs::remove_file(&sock_path);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn test_handle_connection_oversized_message() {
        let tmp = std::env::temp_dir().join(format!("agnos_ipc_oversize_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);
        let sock_path = tmp.join("oversize.sock");
        let _ = std::fs::remove_file(&sock_path);

        let listener = UnixListener::bind(&sock_path).unwrap();
        let (tx, mut rx) = mpsc::channel(10);
        let agent_id = AgentId::new();

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection(stream, tx, agent_id).await;
        });

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        // Send a length that exceeds MAX_MESSAGE_SIZE
        let oversized_len = (MAX_MESSAGE_SIZE + 1).to_be_bytes();

        client.write_all(&oversized_len).await.unwrap();
        drop(client);

        // Handler should close the connection without forwarding any message
        let received = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            rx.recv(),
        ).await;
        // Either timeout or None
        assert!(received.is_err() || received.unwrap().is_none());

        let _ = std::fs::remove_file(&sock_path);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn test_handle_connection_zero_length_then_valid() {
        let tmp = std::env::temp_dir().join(format!("agnos_ipc_zero_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);
        let sock_path = tmp.join("zero.sock");
        let _ = std::fs::remove_file(&sock_path);

        let listener = UnixListener::bind(&sock_path).unwrap();
        let (tx, mut rx) = mpsc::channel(10);
        let agent_id = AgentId::new();

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection(stream, tx, agent_id).await;
        });

        let mut client = UnixStream::connect(&sock_path).await.unwrap();

        // Send zero-length (should be skipped)
        client.write_all(&0u32.to_be_bytes()).await.unwrap();

        // Then send a valid message
        let msg = Message {
            id: "after-zero".to_string(),
            source: "client".to_string(),
            target: "server".to_string(),
            message_type: MessageType::Event,
            payload: serde_json::json!({}),
            timestamp: chrono::Utc::now(),
        };
        let bytes = serde_json::to_vec(&msg).unwrap();
        client.write_all(&(bytes.len() as u32).to_be_bytes()).await.unwrap();
        client.write_all(&bytes).await.unwrap();

        // Read ACK
        let mut ack = [0u8; 1];
        client.read_exact(&mut ack).await.unwrap();
        assert_eq!(ack[0], ACK);

        drop(client);

        let received = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            rx.recv(),
        ).await.unwrap();
        assert!(received.is_some());
        assert_eq!(received.unwrap().id, "after-zero");

        let _ = std::fs::remove_file(&sock_path);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn test_handle_connection_invalid_json() {
        let tmp = std::env::temp_dir().join(format!("agnos_ipc_badjson_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);
        let sock_path = tmp.join("badjson.sock");
        let _ = std::fs::remove_file(&sock_path);

        let listener = UnixListener::bind(&sock_path).unwrap();
        let (tx, mut rx) = mpsc::channel(10);
        let agent_id = AgentId::new();

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection(stream, tx, agent_id).await;
        });

        let mut client = UnixStream::connect(&sock_path).await.unwrap();

        // Send invalid JSON bytes
        let garbage = b"this is not json";
        client.write_all(&(garbage.len() as u32).to_be_bytes()).await.unwrap();
        client.write_all(garbage).await.unwrap();

        // Should receive a NACK_INVALID response
        let mut nack = [0u8; 1];
        client.read_exact(&mut nack).await.unwrap();
        assert_eq!(nack[0], NACK_INVALID);

        drop(client);

        // Handler should not forward any message
        let received = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            rx.recv(),
        ).await;
        assert!(received.is_err() || received.unwrap().is_none());

        let _ = std::fs::remove_file(&sock_path);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_max_message_size_constant() {
        assert_eq!(MAX_MESSAGE_SIZE, 64 * 1024);
    }

    #[test]
    fn test_max_global_subscribers_constant() {
        assert_eq!(MAX_GLOBAL_SUBSCRIBERS, 16);
    }

    #[test]
    fn test_max_concurrent_connections_constant() {
        assert_eq!(MAX_CONCURRENT_CONNECTIONS, 64);
    }

    #[test]
    fn test_ack_nack_constants() {
        assert_eq!(ACK, 0x01);
        assert_eq!(NACK_QUEUE_FULL, 0x02);
        assert_eq!(NACK_INVALID, 0x03);
    }

    #[tokio::test]
    async fn test_handle_connection_backpressure_nack() {
        let tmp = std::env::temp_dir().join(format!("agnos_ipc_bp_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);
        let sock_path = tmp.join("bp.sock");
        let _ = std::fs::remove_file(&sock_path);

        let listener = UnixListener::bind(&sock_path).unwrap();
        // Channel with capacity 1 — second message will trigger NACK
        let (tx, _rx) = mpsc::channel(1);
        let agent_id = AgentId::new();

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection(stream, tx, agent_id).await;
        });

        let mut client = UnixStream::connect(&sock_path).await.unwrap();

        let make_msg = |id: &str| -> Vec<u8> {
            let msg = Message {
                id: id.to_string(),
                source: "client".to_string(),
                target: "server".to_string(),
                message_type: MessageType::Command,
                payload: serde_json::json!({}),
                timestamp: chrono::Utc::now(),
            };
            serde_json::to_vec(&msg).unwrap()
        };

        // First message: should get ACK
        let bytes = make_msg("msg-1");
        client.write_all(&(bytes.len() as u32).to_be_bytes()).await.unwrap();
        client.write_all(&bytes).await.unwrap();
        let mut resp = [0u8; 1];
        client.read_exact(&mut resp).await.unwrap();
        assert_eq!(resp[0], ACK);

        // Second message: queue full, should get NACK
        let bytes = make_msg("msg-2");
        client.write_all(&(bytes.len() as u32).to_be_bytes()).await.unwrap();
        client.write_all(&bytes).await.unwrap();
        client.read_exact(&mut resp).await.unwrap();
        assert_eq!(resp[0], NACK_QUEUE_FULL);

        drop(client);
        let _ = std::fs::remove_file(&sock_path);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ==================================================================
    // New coverage: length-prefix encoding/decoding, MessageBus routing,
    // IPC construction details, multiple global subscribers, try_send
    // ==================================================================

    #[test]
    fn test_length_prefix_encoding_roundtrip() {
        let msg = Message {
            id: "roundtrip".to_string(),
            source: "test".to_string(),
            target: "target".to_string(),
            message_type: MessageType::Command,
            payload: serde_json::json!({"key": "value"}),
            timestamp: chrono::Utc::now(),
        };
        let bytes = serde_json::to_vec(&msg).unwrap();
        let len_bytes = (bytes.len() as u32).to_be_bytes();

        // Decode the length prefix
        let decoded_len = u32::from_be_bytes(len_bytes);
        assert_eq!(decoded_len as usize, bytes.len());

        // Decode the message
        let decoded: Message = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded.id, "roundtrip");
    }

    #[test]
    fn test_length_prefix_zero() {
        let len_bytes = 0u32.to_be_bytes();
        let decoded = u32::from_be_bytes(len_bytes);
        assert_eq!(decoded, 0);
    }

    #[test]
    fn test_length_prefix_max_message_size() {
        let len_bytes = MAX_MESSAGE_SIZE.to_be_bytes();
        let decoded = u32::from_be_bytes(len_bytes);
        assert_eq!(decoded, MAX_MESSAGE_SIZE);
    }

    #[tokio::test]
    async fn test_message_bus_multiple_global_subscribers() {
        let bus = MessageBus::new();
        let mut receivers = Vec::new();

        for _ in 0..3 {
            let (tx, rx) = mpsc::channel(10);
            bus.subscribe_global(tx).await.unwrap();
            receivers.push(rx);
        }

        let msg = Message {
            id: "multi-global".to_string(),
            source: "test".to_string(),
            target: "broadcast".to_string(),
            message_type: MessageType::Event,
            payload: serde_json::json!({}),
            timestamp: chrono::Utc::now(),
        };

        bus.publish(msg).await.unwrap();

        // All global subscribers should receive the message
        for rx in &mut receivers {
            let received = rx.recv().await;
            assert!(received.is_some());
            assert_eq!(received.unwrap().id, "multi-global");
        }
    }

    #[tokio::test]
    async fn test_message_bus_publish_targeted_does_not_broadcast() {
        let bus = MessageBus::new();
        let id1 = AgentId::new();
        let id2 = AgentId::new();
        let (tx1, mut rx1) = mpsc::channel(10);
        let (tx2, mut rx2) = mpsc::channel(10);

        bus.subscribe(id1, tx1).await;
        bus.subscribe(id2, tx2).await;
        bus.register_agent_name(id1, "agent-one").await;
        bus.register_agent_name(id2, "agent-two").await;

        let msg = Message {
            id: "targeted".to_string(),
            source: "test".to_string(),
            target: "agent-one".to_string(),
            message_type: MessageType::Command,
            payload: serde_json::json!({}),
            timestamp: chrono::Utc::now(),
        };

        bus.publish(msg).await.unwrap();

        // Only agent-one should receive it
        let received = rx1.recv().await;
        assert!(received.is_some());

        // agent-two should NOT receive it (use try_recv to avoid blocking)
        let result = rx2.try_recv();
        assert!(result.is_err());
    }

    #[test]
    fn test_agent_ipc_new_returns_receiver() {
        let id = AgentId::new();
        let (ipc, _rx) = AgentIpc::new(id).unwrap();
        assert_eq!(ipc.agent_id, id);
    }

    #[tokio::test]
    async fn test_agent_ipc_send_multiple() {
        let id = AgentId::new();
        let (ipc, mut rx) = AgentIpc::new(id).unwrap();

        for i in 0..3 {
            let msg = Message {
                id: format!("ipc-multi-{}", i),
                source: "test".to_string(),
                target: id.to_string(),
                message_type: MessageType::Event,
                payload: serde_json::json!({}),
                timestamp: chrono::Utc::now(),
            };
            ipc.send(msg).await.unwrap();
        }

        for i in 0..3 {
            let received = rx.recv().await.unwrap();
            assert_eq!(received.id, format!("ipc-multi-{}", i));
        }
    }
}
