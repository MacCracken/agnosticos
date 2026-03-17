//! Task routing — distribution to specific agents and broadcasting.

use anyhow::Result;
use uuid::Uuid;

use agnos_common::{Message, MessageType};

use super::types::Task;
use super::Orchestrator;

impl Orchestrator {
    /// Distribute a task to available agents
    pub(crate) async fn distribute_task(&self, task: &Task) -> Result<()> {
        if task.target_agents.is_empty() {
            // Auto-assign based on capabilities
            self.auto_assign_task(task).await?;
        } else {
            // Send to specific agents
            for agent_id in &task.target_agents {
                if let Some(agent) = self.registry.get(*agent_id) {
                    let message = Message {
                        id: Uuid::new_v4().to_string(),
                        source: "orchestrator".to_string(),
                        target: agent.name,
                        message_type: MessageType::Command,
                        payload: task.payload.clone(),
                        timestamp: chrono::Utc::now(),
                    };

                    self.message_bus
                        .send(message)
                        .await
                        .map_err(|_| anyhow::anyhow!("Failed to send message"))?;
                }
            }
        }

        Ok(())
    }

    /// Broadcast a message to all agents
    pub async fn broadcast(
        &self,
        message_type: MessageType,
        payload: serde_json::Value,
    ) -> Result<()> {
        let agents = self.registry.list_all();

        for agent in agents {
            let message = Message {
                id: Uuid::new_v4().to_string(),
                source: "orchestrator".to_string(),
                target: agent.name,
                message_type,
                payload: payload.clone(),
                timestamp: chrono::Utc::now(),
            };

            self.message_bus
                .send(message)
                .await
                .map_err(|_| anyhow::anyhow!("Failed to broadcast message"))?;
        }

        Ok(())
    }
}
