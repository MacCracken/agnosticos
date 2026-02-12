//! Inter-Process Communication for Agents
//!
//! Handles message passing between agents and the runtime.

use std::collections::HashMap;

use anyhow::{Context, Result};
use tokio::io::AsyncReadExt;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use agnos_common::{AgentId, Message, MessageType};

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

    /// Start listening for incoming connections
    pub async fn start_listening(&self) -> Result<()> {
        // Ensure directory exists
        if let Some(parent) = self.socket_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Remove old socket if it exists
        let _ = tokio::fs::remove_file(&self.socket_path).await;

        let listener = UnixListener::bind(&self.socket_path)
            .with_context(|| format!("Failed to bind to socket: {}", self.socket_path.display()))?;

        info!("Agent {} IPC listening on {}", self.agent_id, self.socket_path.display());

        let tx = self.message_tx.clone();
        let socket_path = self.socket_path.clone();

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let tx = tx.clone();
                        tokio::spawn(handle_connection(stream, tx));
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

/// Handle an incoming connection
async fn handle_connection(mut stream: UnixStream, tx: mpsc::Sender<Message>) {
    let mut buffer = vec![0u8; 4096];

    loop {
        match stream.read(&mut buffer).await {
            Ok(0) => {
                // Connection closed
                break;
            }
            Ok(n) => {
                let data = &buffer[..n];
                
                // Parse message
                match serde_json::from_slice::<Message>(data) {
                    Ok(message) => {
                        debug!("Received message: {:?}", message);
                        if let Err(e) = tx.send(message).await {
                            error!("Failed to forward message: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("Failed to parse message: {}", e);
                    }
                }
            }
            Err(e) => {
                error!("Failed to read from socket: {}", e);
                break;
            }
        }
    }
}

/// IPC bus for routing messages between agents
pub struct MessageBus {
    /// Subscribers by agent ID
    subscribers: RwLock<HashMap<AgentId, mpsc::Sender<Message>>>,
    /// Global subscribers (receive all messages)
    global_subscribers: RwLock<Vec<mpsc::Sender<Message>>>,
}

impl MessageBus {
    /// Create a new message bus
    pub fn new() -> Self {
        Self {
            subscribers: RwLock::new(HashMap::new()),
            global_subscribers: RwLock::new(Vec::new()),
        }
    }

    /// Subscribe an agent to receive messages
    pub async fn subscribe(&self, agent_id: AgentId, sender: mpsc::Sender<Message>) {
        self.subscribers.write().await.insert(agent_id, sender);
    }

    /// Unsubscribe an agent
    pub async fn unsubscribe(&self, agent_id: AgentId) {
        self.subscribers.write().await.remove(&agent_id);
    }

    /// Subscribe to all messages (for monitoring/debugging)
    pub async fn subscribe_global(&self, sender: mpsc::Sender<Message>) {
        self.global_subscribers.write().await.push(sender);
    }

    /// Publish a message
    pub async fn publish(&self, message: Message) -> Result<()> {
        // Send to specific target if specified
        if message.target != "*" && message.target != "broadcast" {
            // TODO: Look up agent ID by name
            // For now, broadcast to all
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
}

impl Default for MessageBus {
    fn default() -> Self {
        Self::new()
    }
}
