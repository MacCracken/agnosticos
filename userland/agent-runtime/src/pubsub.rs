//! Topic-based pub/sub for agent-to-agent communication.
//!
//! Provides structured message passing on top of the existing `MessageBus`.
//! Agents subscribe to named topics and publish typed messages.
//! Supports request/reply via correlation IDs.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, warn};

use agnos_common::AgentId;

/// A pub/sub message with topic routing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicMessage {
    /// Topic name (e.g., "file.indexed", "task.completed", "llm.result").
    pub topic: String,
    /// Sender agent ID.
    pub sender: AgentId,
    /// Message payload (JSON).
    pub payload: serde_json::Value,
    /// Optional correlation ID for request/reply patterns.
    #[serde(default)]
    pub correlation_id: Option<String>,
    /// Optional reply-to topic (for request/reply).
    #[serde(default)]
    pub reply_to: Option<String>,
    /// Timestamp (epoch millis).
    pub timestamp: u64,
}

impl TopicMessage {
    /// Create a new topic message.
    pub fn new(topic: impl Into<String>, sender: AgentId, payload: serde_json::Value) -> Self {
        Self {
            topic: topic.into(),
            sender,
            payload,
            correlation_id: None,
            reply_to: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        }
    }

    /// Create a request message (sets correlation_id and reply_to).
    pub fn request(
        topic: impl Into<String>,
        sender: AgentId,
        payload: serde_json::Value,
        reply_topic: impl Into<String>,
    ) -> Self {
        Self {
            topic: topic.into(),
            sender,
            payload,
            correlation_id: Some(uuid::Uuid::new_v4().to_string()),
            reply_to: Some(reply_topic.into()),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        }
    }

    /// Create a reply to a request message.
    pub fn reply(&self, sender: AgentId, payload: serde_json::Value) -> Option<Self> {
        let reply_topic = self.reply_to.as_ref()?;
        Some(Self {
            topic: reply_topic.clone(),
            sender,
            payload,
            correlation_id: self.correlation_id.clone(),
            reply_to: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        })
    }
}

/// Topic-based pub/sub broker for inter-agent communication.
pub struct TopicBroker {
    /// Topic → set of subscribed agent IDs.
    subscriptions: RwLock<HashMap<String, HashSet<AgentId>>>,
    /// Agent ID → message sender channel.
    channels: RwLock<HashMap<AgentId, mpsc::Sender<TopicMessage>>>,
    /// Wildcard subscriptions (e.g., "file.*" subscribes to all file.* topics).
    wildcard_subs: RwLock<HashMap<String, HashSet<AgentId>>>,
    /// Message log for debugging (last N messages).
    message_log: RwLock<Vec<TopicMessage>>,
    max_log_size: usize,
}

impl TopicBroker {
    pub fn new() -> Self {
        Self {
            subscriptions: RwLock::new(HashMap::new()),
            channels: RwLock::new(HashMap::new()),
            wildcard_subs: RwLock::new(HashMap::new()),
            message_log: RwLock::new(Vec::new()),
            max_log_size: 500,
        }
    }

    /// Register an agent's message channel.
    pub async fn register(&self, agent_id: AgentId, sender: mpsc::Sender<TopicMessage>) {
        self.channels.write().await.insert(agent_id, sender);
        debug!("Agent {} registered with topic broker", agent_id);
    }

    /// Unregister an agent (removes all subscriptions and channel).
    pub async fn unregister(&self, agent_id: AgentId) {
        self.channels.write().await.remove(&agent_id);

        // Remove from all topic subscriptions
        let mut subs = self.subscriptions.write().await;
        for subscribers in subs.values_mut() {
            subscribers.remove(&agent_id);
        }

        let mut wildcards = self.wildcard_subs.write().await;
        for subscribers in wildcards.values_mut() {
            subscribers.remove(&agent_id);
        }

        debug!("Agent {} unregistered from topic broker", agent_id);
    }

    /// Subscribe an agent to a topic.
    /// Supports wildcard suffix: "file.*" matches "file.created", "file.deleted", etc.
    pub async fn subscribe(&self, agent_id: AgentId, topic: &str) {
        if let Some(prefix) = topic.strip_suffix(".*") {
            let mut wildcards = self.wildcard_subs.write().await;
            wildcards
                .entry(prefix.to_string())
                .or_default()
                .insert(agent_id);
        } else {
            let mut subs = self.subscriptions.write().await;
            subs.entry(topic.to_string()).or_default().insert(agent_id);
        }
        debug!("Agent {} subscribed to '{}'", agent_id, topic);
    }

    /// Unsubscribe an agent from a topic.
    pub async fn unsubscribe(&self, agent_id: AgentId, topic: &str) {
        if let Some(prefix) = topic.strip_suffix(".*") {
            let mut wildcards = self.wildcard_subs.write().await;
            if let Some(subs) = wildcards.get_mut(prefix) {
                subs.remove(&agent_id);
            }
        } else {
            let mut subs = self.subscriptions.write().await;
            if let Some(subscribers) = subs.get_mut(topic) {
                subscribers.remove(&agent_id);
            }
        }
    }

    /// Publish a message to a topic. Delivers to all subscribed agents.
    /// Returns the number of agents the message was delivered to.
    pub async fn publish(&self, message: TopicMessage) -> usize {
        let topic = &message.topic;
        let sender = message.sender;

        // Collect target agent IDs
        let mut targets = HashSet::new();

        // Exact topic matches
        {
            let subs = self.subscriptions.read().await;
            if let Some(subscribers) = subs.get(topic) {
                targets.extend(subscribers);
            }
        }

        // Wildcard matches
        {
            let wildcards = self.wildcard_subs.read().await;
            for (prefix, subscribers) in wildcards.iter() {
                if topic.starts_with(prefix) {
                    targets.extend(subscribers);
                }
            }
        }

        // Don't send to the sender
        targets.remove(&sender);

        // Deliver
        let channels = self.channels.read().await;
        let mut delivered = 0;

        for &agent_id in &targets {
            if let Some(tx) = channels.get(&agent_id) {
                match tx.try_send(message.clone()) {
                    Ok(()) => delivered += 1,
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        warn!(
                            "Agent {} topic queue full, dropping message on '{}'",
                            agent_id, topic
                        );
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        debug!("Agent {} channel closed", agent_id);
                    }
                }
            }
        }

        // Log
        {
            let mut log = self.message_log.write().await;
            if log.len() >= self.max_log_size {
                log.remove(0);
            }
            log.push(message);
        }

        delivered
    }

    /// List all topics with subscriber counts.
    pub async fn list_topics(&self) -> Vec<(String, usize)> {
        let subs = self.subscriptions.read().await;
        subs.iter()
            .map(|(topic, subscribers)| (topic.clone(), subscribers.len()))
            .collect()
    }

    /// Get all topics an agent is subscribed to.
    pub async fn agent_subscriptions(&self, agent_id: AgentId) -> Vec<String> {
        let mut topics = Vec::new();

        let subs = self.subscriptions.read().await;
        for (topic, subscribers) in subs.iter() {
            if subscribers.contains(&agent_id) {
                topics.push(topic.clone());
            }
        }

        let wildcards = self.wildcard_subs.read().await;
        for (prefix, subscribers) in wildcards.iter() {
            if subscribers.contains(&agent_id) {
                topics.push(format!("{}.*", prefix));
            }
        }

        topics
    }

    /// Get recent messages (for debugging).
    pub async fn recent_messages(&self, count: usize) -> Vec<TopicMessage> {
        let log = self.message_log.read().await;
        let start = log.len().saturating_sub(count);
        log[start..].to_vec()
    }
}

impl Default for TopicBroker {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn agent() -> AgentId {
        AgentId::new()
    }

    #[tokio::test]
    async fn test_broker_new() {
        let broker = TopicBroker::new();
        let topics = broker.list_topics().await;
        assert!(topics.is_empty());
    }

    #[tokio::test]
    async fn test_subscribe_and_publish() {
        let broker = TopicBroker::new();
        let sender = agent();
        let receiver = agent();

        let (tx, mut rx) = mpsc::channel(10);
        broker.register(receiver, tx).await;
        broker.subscribe(receiver, "events.test").await;

        let msg = TopicMessage::new("events.test", sender, serde_json::json!({"key": "value"}));
        let delivered = broker.publish(msg).await;

        assert_eq!(delivered, 1);
        let received = rx.try_recv().unwrap();
        assert_eq!(received.topic, "events.test");
        assert_eq!(received.payload["key"], "value");
    }

    #[tokio::test]
    async fn test_wildcard_subscription() {
        let broker = TopicBroker::new();
        let sender = agent();
        let receiver = agent();

        let (tx, mut rx) = mpsc::channel(10);
        broker.register(receiver, tx).await;
        broker.subscribe(receiver, "file.*").await;

        let msg1 = TopicMessage::new("file.created", sender, serde_json::json!({}));
        let msg2 = TopicMessage::new("file.deleted", sender, serde_json::json!({}));
        let msg3 = TopicMessage::new("task.done", sender, serde_json::json!({}));

        assert_eq!(broker.publish(msg1).await, 1);
        assert_eq!(broker.publish(msg2).await, 1);
        assert_eq!(broker.publish(msg3).await, 0); // no match

        assert_eq!(rx.try_recv().unwrap().topic, "file.created");
        assert_eq!(rx.try_recv().unwrap().topic, "file.deleted");
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_no_self_delivery() {
        let broker = TopicBroker::new();
        let a = agent();

        let (tx, mut rx) = mpsc::channel(10);
        broker.register(a, tx).await;
        broker.subscribe(a, "test").await;

        let msg = TopicMessage::new("test", a, serde_json::json!({}));
        let delivered = broker.publish(msg).await;

        assert_eq!(delivered, 0);
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let broker = TopicBroker::new();
        let sender = agent();
        let r1 = agent();
        let r2 = agent();

        let (tx1, mut rx1) = mpsc::channel(10);
        let (tx2, mut rx2) = mpsc::channel(10);

        broker.register(r1, tx1).await;
        broker.register(r2, tx2).await;
        broker.subscribe(r1, "broadcast").await;
        broker.subscribe(r2, "broadcast").await;

        let msg = TopicMessage::new("broadcast", sender, serde_json::json!("hello"));
        let delivered = broker.publish(msg).await;

        assert_eq!(delivered, 2);
        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());
    }

    #[tokio::test]
    async fn test_unsubscribe() {
        let broker = TopicBroker::new();
        let sender = agent();
        let receiver = agent();

        let (tx, mut rx) = mpsc::channel(10);
        broker.register(receiver, tx).await;
        broker.subscribe(receiver, "events").await;
        broker.unsubscribe(receiver, "events").await;

        let msg = TopicMessage::new("events", sender, serde_json::json!({}));
        assert_eq!(broker.publish(msg).await, 0);
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_unregister_cleans_up() {
        let broker = TopicBroker::new();
        let a = agent();

        let (tx, _rx) = mpsc::channel(10);
        broker.register(a, tx).await;
        broker.subscribe(a, "test").await;
        broker.subscribe(a, "events.*").await;

        broker.unregister(a).await;

        let subs = broker.agent_subscriptions(a).await;
        assert!(subs.is_empty());
    }

    #[tokio::test]
    async fn test_request_reply() {
        let broker = TopicBroker::new();
        let client = agent();
        let server = agent();

        let (server_tx, mut server_rx) = mpsc::channel(10);
        let (client_tx, mut client_rx) = mpsc::channel(10);

        broker.register(server, server_tx).await;
        broker.register(client, client_tx).await;
        broker.subscribe(server, "rpc.add").await;
        broker.subscribe(client, "rpc.add.reply").await;

        // Client sends request
        let req = TopicMessage::request(
            "rpc.add",
            client,
            serde_json::json!({"a": 1, "b": 2}),
            "rpc.add.reply",
        );
        broker.publish(req).await;

        // Server receives and replies
        let received = server_rx.try_recv().unwrap();
        assert_eq!(received.topic, "rpc.add");
        assert!(received.correlation_id.is_some());

        let reply = received
            .reply(server, serde_json::json!({"result": 3}))
            .unwrap();
        broker.publish(reply).await;

        // Client receives reply
        let resp = client_rx.try_recv().unwrap();
        assert_eq!(resp.topic, "rpc.add.reply");
        assert_eq!(resp.payload["result"], 3);
        assert_eq!(resp.correlation_id, received.correlation_id);
    }

    #[tokio::test]
    async fn test_list_topics() {
        let broker = TopicBroker::new();
        let a = agent();

        let (tx, _rx) = mpsc::channel(10);
        broker.register(a, tx).await;
        broker.subscribe(a, "alpha").await;
        broker.subscribe(a, "beta").await;

        let topics = broker.list_topics().await;
        assert_eq!(topics.len(), 2);
    }

    #[tokio::test]
    async fn test_agent_subscriptions() {
        let broker = TopicBroker::new();
        let a = agent();

        let (tx, _rx) = mpsc::channel(10);
        broker.register(a, tx).await;
        broker.subscribe(a, "events").await;
        broker.subscribe(a, "logs.*").await;

        let subs = broker.agent_subscriptions(a).await;
        assert_eq!(subs.len(), 2);
        assert!(subs.contains(&"events".to_string()));
        assert!(subs.contains(&"logs.*".to_string()));
    }

    #[tokio::test]
    async fn test_full_queue_drops_message() {
        let broker = TopicBroker::new();
        let sender = agent();
        let receiver = agent();

        let (tx, _rx) = mpsc::channel(1); // tiny buffer
        broker.register(receiver, tx).await;
        broker.subscribe(receiver, "flood").await;

        // Fill the queue
        broker
            .publish(TopicMessage::new("flood", sender, serde_json::json!({})))
            .await;

        // This should drop (queue full), not panic
        let delivered = broker
            .publish(TopicMessage::new("flood", sender, serde_json::json!({})))
            .await;
        assert_eq!(delivered, 0);
    }

    #[tokio::test]
    async fn test_message_log() {
        let broker = TopicBroker::new();
        let a = agent();

        broker
            .publish(TopicMessage::new("test", a, serde_json::json!({})))
            .await;

        let recent = broker.recent_messages(10).await;
        assert_eq!(recent.len(), 1);
    }

    #[test]
    fn test_topic_message_new() {
        let id = AgentId::new();
        let msg = TopicMessage::new("test.topic", id, serde_json::json!(42));
        assert_eq!(msg.topic, "test.topic");
        assert_eq!(msg.sender, id);
        assert!(msg.correlation_id.is_none());
        assert!(msg.reply_to.is_none());
        assert!(msg.timestamp > 0);
    }

    #[test]
    fn test_topic_message_request() {
        let id = AgentId::new();
        let msg = TopicMessage::request("rpc.call", id, serde_json::json!({}), "rpc.reply");
        assert!(msg.correlation_id.is_some());
        assert_eq!(msg.reply_to.as_deref(), Some("rpc.reply"));
    }

    #[test]
    fn test_topic_message_reply() {
        let id1 = AgentId::new();
        let id2 = AgentId::new();
        let req = TopicMessage::request("rpc.call", id1, serde_json::json!({}), "rpc.reply");
        let reply = req.reply(id2, serde_json::json!("ok")).unwrap();
        assert_eq!(reply.topic, "rpc.reply");
        assert_eq!(reply.correlation_id, req.correlation_id);
        assert!(reply.reply_to.is_none());
    }

    #[test]
    fn test_topic_message_no_reply_without_reply_to() {
        let id = AgentId::new();
        let msg = TopicMessage::new("test", id, serde_json::json!({}));
        assert!(msg.reply(id, serde_json::json!({})).is_none());
    }

    #[test]
    fn test_topic_message_serialization() {
        let msg = TopicMessage::new("test", AgentId::new(), serde_json::json!({"data": true}));
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: TopicMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.topic, "test");
        assert_eq!(parsed.payload["data"], true);
    }
}
