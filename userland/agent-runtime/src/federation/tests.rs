//! Tests for the federation module.

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::net::SocketAddr;

    use chrono::{Duration, Utc};

    use crate::federation::{
        AgentPlacement, AgentRequirements, FederatedVectorStore, FederationCluster,
        FederationConfig, FederationNode, NodeCapabilities, NodeRole, NodeScorer, NodeStatus,
        RemoteSearchResult, SchedulingStrategy, VectorReplicationStrategy, VectorSyncEntry,
        VectorSyncMessage, VoteResponse,
    };

    fn make_node(name: &str, addr: &str) -> FederationNode {
        FederationNode::new(
            name.to_string(),
            addr.parse().unwrap(),
            NodeCapabilities::default(),
        )
    }

    fn make_node_with_caps(name: &str, addr: &str, cpu: u32, mem: u64, gpu: u32) -> FederationNode {
        FederationNode::new(
            name.to_string(),
            addr.parse().unwrap(),
            NodeCapabilities {
                cpu_cores: cpu,
                memory_mb: mem,
                gpu_count: gpu,
            },
        )
    }

    // -------------------------------------------------------------------
    // Node registration
    // -------------------------------------------------------------------

    #[test]
    fn test_node_creation() {
        let node = make_node("test-node", "127.0.0.1:8092");
        assert_eq!(node.name, "test-node");
        assert_eq!(node.role, NodeRole::Follower);
        assert_eq!(node.status, NodeStatus::Online);
        assert_eq!(node.current_term, 0);
        assert!(node.voted_for.is_none());
    }

    #[test]
    fn test_cluster_creation() {
        let node = make_node("node-1", "127.0.0.1:8092");
        let cluster = FederationCluster::new(node);
        assert_eq!(cluster.node_count(), 1);
        assert!(cluster.coordinator_id().is_none());
    }

    #[test]
    fn test_register_node() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);

        let peer = make_node("node-2", "127.0.0.2:8092");
        cluster.register_node(peer).unwrap();
        assert_eq!(cluster.node_count(), 2);
    }

    #[test]
    fn test_register_duplicate_node_fails() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let local_id = local.node_id.clone();
        let mut cluster = FederationCluster::new(local);

        // Try to register with the same ID
        let mut dup = make_node("node-1-dup", "127.0.0.1:8093");
        dup.node_id = local_id;
        assert!(cluster.register_node(dup).is_err());
    }

    #[test]
    fn test_remove_node() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);

        let peer = make_node("node-2", "127.0.0.2:8092");
        let peer_id = peer.node_id.clone();
        cluster.register_node(peer).unwrap();
        assert_eq!(cluster.node_count(), 2);

        cluster.remove_node(&peer_id).unwrap();
        assert_eq!(cluster.node_count(), 1);
    }

    #[test]
    fn test_remove_local_node_fails() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let local_id = local.node_id.clone();
        let mut cluster = FederationCluster::new(local);
        assert!(cluster.remove_node(&local_id).is_err());
    }

    #[test]
    fn test_remove_unknown_node_fails() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);
        assert!(cluster.remove_node("nonexistent").is_err());
    }

    #[test]
    fn test_get_node() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let local_id = local.node_id.clone();
        let cluster = FederationCluster::new(local);

        let node = cluster.get_node(&local_id).unwrap();
        assert_eq!(node.name, "node-1");
        assert!(cluster.get_node("nonexistent").is_none());
    }

    // -------------------------------------------------------------------
    // Heartbeat tracking
    // -------------------------------------------------------------------

    #[test]
    fn test_record_heartbeat() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let local_id = local.node_id.clone();
        let mut cluster = FederationCluster::new(local);

        let before = Utc::now();
        cluster.record_heartbeat(&local_id).unwrap();
        let after = Utc::now();

        let node = cluster.get_node(&local_id).unwrap();
        assert!(node.last_heartbeat >= before);
        assert!(node.last_heartbeat <= after);
    }

    #[test]
    fn test_heartbeat_unknown_node_fails() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);
        assert!(cluster.record_heartbeat("nonexistent").is_err());
    }

    #[test]
    fn test_heartbeat_recovers_suspect_node() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);

        let mut peer = make_node("node-2", "127.0.0.2:8092");
        peer.status = NodeStatus::Suspect;
        let peer_id = peer.node_id.clone();
        cluster.register_node(peer).unwrap();

        cluster.record_heartbeat(&peer_id).unwrap();
        assert_eq!(
            cluster.get_node(&peer_id).unwrap().status,
            NodeStatus::Online
        );
    }

    // -------------------------------------------------------------------
    // Health transitions
    // -------------------------------------------------------------------

    #[test]
    fn test_health_online_stays_online() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);
        cluster.check_health();
        let nodes = cluster.get_live_nodes();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].status, NodeStatus::Online);
    }

    #[test]
    fn test_health_online_to_suspect() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);

        let peer = make_node("node-2", "127.0.0.2:8092");
        let peer_id = peer.node_id.clone();
        cluster.register_node(peer).unwrap();

        // Set peer heartbeat to 20 seconds ago
        let old_time = Utc::now() - Duration::seconds(20);
        cluster.set_heartbeat_time(&peer_id, old_time).unwrap();

        cluster.check_health();
        assert_eq!(
            cluster.get_node(&peer_id).unwrap().status,
            NodeStatus::Suspect
        );
    }

    #[test]
    fn test_health_online_to_dead() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);

        let peer = make_node("node-2", "127.0.0.2:8092");
        let peer_id = peer.node_id.clone();
        cluster.register_node(peer).unwrap();

        // Set peer heartbeat to 35 seconds ago
        let old_time = Utc::now() - Duration::seconds(35);
        cluster.set_heartbeat_time(&peer_id, old_time).unwrap();

        cluster.check_health();
        assert_eq!(cluster.get_node(&peer_id).unwrap().status, NodeStatus::Dead);
    }

    #[test]
    fn test_health_suspect_to_dead() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);

        let mut peer = make_node("node-2", "127.0.0.2:8092");
        peer.status = NodeStatus::Suspect;
        let peer_id = peer.node_id.clone();
        cluster.register_node(peer).unwrap();

        // Set heartbeat well past dead threshold
        let old_time = Utc::now() - Duration::seconds(45);
        cluster.set_heartbeat_time(&peer_id, old_time).unwrap();

        cluster.check_health();
        assert_eq!(cluster.get_node(&peer_id).unwrap().status, NodeStatus::Dead);
    }

    #[test]
    fn test_get_live_nodes_filters_dead() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);

        let peer = make_node("node-2", "127.0.0.2:8092");
        let peer_id = peer.node_id.clone();
        cluster.register_node(peer).unwrap();

        // Mark peer dead via old heartbeat
        let old_time = Utc::now() - Duration::seconds(60);
        cluster.set_heartbeat_time(&peer_id, old_time).unwrap();
        cluster.check_health();

        let live = cluster.get_live_nodes();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].name, "node-1");
    }

    // -------------------------------------------------------------------
    // Coordinator election
    // -------------------------------------------------------------------

    #[test]
    fn test_single_node_election() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let local_id = local.node_id.clone();
        let mut cluster = FederationCluster::new(local);

        let term = cluster.start_election().unwrap();
        assert_eq!(term, 1);
        assert_eq!(cluster.coordinator_id(), Some(local_id.as_str()));
        assert_eq!(
            cluster.get_node(&local_id).unwrap().role,
            NodeRole::Coordinator
        );
    }

    #[test]
    fn test_two_node_election_with_vote() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let local_id = local.node_id.clone();
        let mut cluster = FederationCluster::new(local);

        let peer = make_node("node-2", "127.0.0.2:8092");
        let peer_id = peer.node_id.clone();
        cluster.register_node(peer).unwrap();

        let term = cluster.start_election().unwrap();
        assert_eq!(term, 1);

        // Candidate has 1 self-vote, needs 2 (majority of 2 = 2)
        assert!(
            cluster.coordinator_id().is_none()
                || cluster.coordinator_id() == Some(local_id.as_str())
        );

        // Simulate peer voting for local
        let vote = VoteResponse {
            voter_id: peer_id.clone(),
            term: 1,
            granted: true,
        };
        let has_majority = cluster.receive_vote(&local_id, vote);
        assert!(has_majority);

        cluster.become_coordinator(&local_id).unwrap();
        assert_eq!(cluster.coordinator_id(), Some(local_id.as_str()));
        assert_eq!(
            cluster.get_node(&local_id).unwrap().role,
            NodeRole::Coordinator
        );
        assert_eq!(cluster.get_node(&peer_id).unwrap().role, NodeRole::Follower);
    }

    #[test]
    fn test_three_node_election_majority() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let local_id = local.node_id.clone();
        let mut cluster = FederationCluster::new(local);

        let peer2 = make_node("node-2", "127.0.0.2:8092");
        let peer2_id = peer2.node_id.clone();
        cluster.register_node(peer2).unwrap();

        let peer3 = make_node("node-3", "127.0.0.3:8092");
        let _peer3_id = peer3.node_id.clone();
        cluster.register_node(peer3).unwrap();

        cluster.start_election().unwrap();

        // Self-vote gives 1 out of 3 — not majority
        assert!(cluster.coordinator_id().is_none());

        // One more vote gives majority (2 of 3)
        let vote = VoteResponse {
            voter_id: peer2_id.clone(),
            term: 1,
            granted: true,
        };
        let has_majority = cluster.receive_vote(&local_id, vote);
        assert!(has_majority);
    }

    #[test]
    fn test_competing_candidates_higher_term_wins() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let local_id = local.node_id.clone();
        let mut cluster = FederationCluster::new(local);

        let peer = make_node("node-2", "127.0.0.2:8092");
        let _peer_id = peer.node_id.clone();
        cluster.register_node(peer).unwrap();

        // Local starts election at term 1
        cluster.start_election().unwrap();
        assert_eq!(cluster.get_node(&local_id).unwrap().current_term, 1);

        // Peer requests vote at term 2 — local should step down and grant
        let response = cluster.receive_vote_request("external-candidate", 2);
        assert!(response.granted);
        assert_eq!(
            cluster.get_node(&local_id).unwrap().role,
            NodeRole::Follower
        );
        assert_eq!(cluster.get_node(&local_id).unwrap().current_term, 2);
    }

    #[test]
    fn test_stale_term_vote_rejected() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);

        // Advance term
        cluster.start_election().unwrap();

        // Request vote with stale term 0
        let response = cluster.receive_vote_request("stale-candidate", 0);
        assert!(!response.granted);
        assert_eq!(response.term, 1);
    }

    #[test]
    fn test_double_vote_same_term_denied() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);

        // Vote for candidate-A in term 1
        let resp1 = cluster.receive_vote_request("candidate-a", 1);
        assert!(resp1.granted);

        // Try to vote for candidate-B in same term 1
        let resp2 = cluster.receive_vote_request("candidate-b", 1);
        assert!(!resp2.granted);
    }

    #[test]
    fn test_vote_for_same_candidate_twice_ok() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);

        let resp1 = cluster.receive_vote_request("candidate-a", 1);
        assert!(resp1.granted);

        // Same candidate, same term — should still be granted
        let resp2 = cluster.receive_vote_request("candidate-a", 1);
        assert!(resp2.granted);
    }

    #[test]
    fn test_term_advancement_on_election() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let local_id = local.node_id.clone();
        let mut cluster = FederationCluster::new(local);

        let t1 = cluster.start_election().unwrap();
        assert_eq!(t1, 1);

        // Start another election
        // Reset role to follower first
        cluster.step_down(&local_id, 1).unwrap();
        let t2 = cluster.start_election().unwrap();
        assert_eq!(t2, 2);
    }

    #[test]
    fn test_step_down_clears_coordinator() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let local_id = local.node_id.clone();
        let mut cluster = FederationCluster::new(local);

        cluster.start_election().unwrap();
        assert!(cluster.coordinator_id().is_some());

        cluster.step_down(&local_id, 2).unwrap();
        assert!(cluster.coordinator_id().is_none());
        assert_eq!(
            cluster.get_node(&local_id).unwrap().role,
            NodeRole::Follower
        );
    }

    #[test]
    fn test_remove_coordinator_clears_coordinator_id() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);

        let peer = make_node("node-2", "127.0.0.2:8092");
        let peer_id = peer.node_id.clone();
        cluster.register_node(peer).unwrap();

        // Make peer the coordinator
        cluster.get_node_mut(&peer_id).unwrap().current_term = 1;
        cluster.become_coordinator(&peer_id).unwrap();
        assert_eq!(cluster.coordinator_id(), Some(peer_id.as_str()));

        cluster.remove_node(&peer_id).unwrap();
        assert!(cluster.coordinator_id().is_none());
    }

    #[test]
    fn test_denied_vote_not_counted() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let local_id = local.node_id.clone();
        let mut cluster = FederationCluster::new(local);

        let peer = make_node("node-2", "127.0.0.2:8092");
        let peer_id = peer.node_id.clone();
        cluster.register_node(peer).unwrap();

        cluster.start_election().unwrap();

        let vote = VoteResponse {
            voter_id: peer_id,
            term: 1,
            granted: false,
        };
        let has_majority = cluster.receive_vote(&local_id, vote);
        assert!(!has_majority);
    }

    // -------------------------------------------------------------------
    // Node scoring
    // -------------------------------------------------------------------

    #[test]
    fn test_score_node_basic() {
        let node = make_node_with_caps("node-1", "127.0.0.1:8092", 8, 16384, 0);
        let scorer = NodeScorer::new();
        let reqs = AgentRequirements::default();

        let score = scorer.score_node(&node, &reqs);
        assert!(score.total_score > 0.0);
        assert!(score.total_score <= 1.0);
    }

    #[test]
    fn test_score_insufficient_cpu() {
        let node = make_node_with_caps("node-1", "127.0.0.1:8092", 1, 16384, 0);
        let scorer = NodeScorer::new();
        let reqs = AgentRequirements {
            cpu_cores: 4,
            ..Default::default()
        };

        let score = scorer.score_node(&node, &reqs);
        assert_eq!(score.breakdown.resource_headroom, 0.0);
    }

    #[test]
    fn test_score_insufficient_memory() {
        let node = make_node_with_caps("node-1", "127.0.0.1:8092", 8, 256, 0);
        let scorer = NodeScorer::new();
        let reqs = AgentRequirements {
            memory_mb: 512,
            ..Default::default()
        };

        let score = scorer.score_node(&node, &reqs);
        assert_eq!(score.breakdown.resource_headroom, 0.0);
    }

    #[test]
    fn test_score_gpu_required_but_absent() {
        let node = make_node_with_caps("node-1", "127.0.0.1:8092", 8, 16384, 0);
        let scorer = NodeScorer::new();
        let reqs = AgentRequirements {
            gpu_required: true,
            ..Default::default()
        };

        let score = scorer.score_node(&node, &reqs);
        assert_eq!(score.breakdown.resource_headroom, 0.0);
    }

    #[test]
    fn test_score_locality_preferred_match() {
        let node = make_node_with_caps("gpu-node", "127.0.0.1:8092", 8, 16384, 2);
        let scorer = NodeScorer::new();
        let reqs = AgentRequirements {
            preferred_node: Some("gpu-node".to_string()),
            ..Default::default()
        };

        let score = scorer.score_node(&node, &reqs);
        assert_eq!(score.breakdown.locality, 1.0);
    }

    #[test]
    fn test_score_locality_preferred_mismatch() {
        let node = make_node_with_caps("cpu-node", "127.0.0.1:8092", 8, 16384, 0);
        let scorer = NodeScorer::new();
        let reqs = AgentRequirements {
            preferred_node: Some("gpu-node".to_string()),
            ..Default::default()
        };

        let score = scorer.score_node(&node, &reqs);
        assert_eq!(score.breakdown.locality, 0.0);
    }

    #[test]
    fn test_score_locality_no_preference() {
        let node = make_node_with_caps("node-1", "127.0.0.1:8092", 8, 16384, 0);
        let scorer = NodeScorer::new();
        let reqs = AgentRequirements::default();

        let score = scorer.score_node(&node, &reqs);
        assert_eq!(score.breakdown.locality, 0.5);
    }

    #[test]
    fn test_score_load_balance() {
        let node = make_node_with_caps("node-1", "127.0.0.1:8092", 8, 16384, 0);
        let mut scorer = NodeScorer::new();

        let reqs = AgentRequirements::default();

        // No load — full score
        let score0 = scorer.score_node(&node, &reqs);
        assert_eq!(score0.breakdown.load_balance, 1.0);

        // Some load
        scorer.set_load(&node.node_id, 3);
        let score3 = scorer.score_node(&node, &reqs);
        assert!(score3.breakdown.load_balance < score0.breakdown.load_balance);
    }

    #[test]
    fn test_score_affinity_match() {
        let node = make_node_with_caps("node-1", "127.0.0.1:8092", 8, 16384, 0);
        let scorer = NodeScorer::new();
        let reqs = AgentRequirements {
            affinity_nodes: vec!["node-1".to_string()],
            ..Default::default()
        };

        let score = scorer.score_node(&node, &reqs);
        assert_eq!(score.breakdown.affinity, 1.0);
    }

    #[test]
    fn test_score_affinity_no_match() {
        let node = make_node_with_caps("node-1", "127.0.0.1:8092", 8, 16384, 0);
        let scorer = NodeScorer::new();
        let reqs = AgentRequirements {
            affinity_nodes: vec!["node-2".to_string()],
            ..Default::default()
        };

        let score = scorer.score_node(&node, &reqs);
        assert_eq!(score.breakdown.affinity, 0.0);
    }

    // -------------------------------------------------------------------
    // Agent placement
    // -------------------------------------------------------------------

    #[test]
    fn test_place_agent_single_node() {
        let local = make_node_with_caps("node-1", "127.0.0.1:8092", 8, 16384, 0);
        let local_id = local.node_id.clone();
        let cluster = FederationCluster::new(local);

        let scorer = NodeScorer::new();
        let placement = AgentPlacement::new(scorer);
        let reqs = AgentRequirements::default();

        let result = placement.place_agent(&cluster, &reqs).unwrap();
        assert_eq!(result.node_id, local_id);
    }

    #[test]
    fn test_place_agent_prefers_better_node() {
        let local = make_node_with_caps("small-node", "127.0.0.1:8092", 2, 2048, 0);
        let mut cluster = FederationCluster::new(local);

        let big = make_node_with_caps("big-node", "127.0.0.2:8092", 16, 65536, 0);
        let big_id = big.node_id.clone();
        cluster.register_node(big).unwrap();

        let scorer = NodeScorer::new();
        let placement = AgentPlacement::new(scorer);
        let reqs = AgentRequirements {
            cpu_cores: 2,
            memory_mb: 1024,
            ..Default::default()
        };

        let result = placement.place_agent(&cluster, &reqs).unwrap();
        // Big node should score higher due to more headroom
        assert_eq!(result.node_id, big_id);
    }

    #[test]
    fn test_place_agent_no_eligible_nodes() {
        let local = make_node_with_caps("tiny", "127.0.0.1:8092", 1, 512, 0);
        let cluster = FederationCluster::new(local);

        let scorer = NodeScorer::new();
        let placement = AgentPlacement::new(scorer);
        let reqs = AgentRequirements {
            cpu_cores: 8,
            memory_mb: 32768,
            ..Default::default()
        };

        assert!(placement.place_agent(&cluster, &reqs).is_err());
    }

    #[test]
    fn test_place_agent_respects_gpu_requirement() {
        let cpu_node = make_node_with_caps("cpu-node", "127.0.0.1:8092", 16, 65536, 0);
        let mut cluster = FederationCluster::new(cpu_node);

        let gpu_node = make_node_with_caps("gpu-node", "127.0.0.2:8092", 8, 32768, 2);
        let gpu_id = gpu_node.node_id.clone();
        cluster.register_node(gpu_node).unwrap();

        let scorer = NodeScorer::new();
        let placement = AgentPlacement::new(scorer);
        let reqs = AgentRequirements {
            gpu_required: true,
            ..Default::default()
        };

        let result = placement.place_agent(&cluster, &reqs).unwrap();
        assert_eq!(result.node_id, gpu_id);
    }

    #[test]
    fn test_place_agent_dead_nodes_excluded() {
        let local = make_node_with_caps("node-1", "127.0.0.1:8092", 4, 8192, 0);
        let local_id = local.node_id.clone();
        let mut cluster = FederationCluster::new(local);

        let peer = make_node_with_caps("node-2", "127.0.0.2:8092", 16, 65536, 0);
        let peer_id = peer.node_id.clone();
        cluster.register_node(peer).unwrap();

        // Kill peer
        let old_time = Utc::now() - Duration::seconds(60);
        cluster.set_heartbeat_time(&peer_id, old_time).unwrap();
        cluster.check_health();

        let scorer = NodeScorer::new();
        let placement = AgentPlacement::new(scorer);
        let reqs = AgentRequirements::default();

        let result = placement.place_agent(&cluster, &reqs).unwrap();
        assert_eq!(result.node_id, local_id);
    }

    // -------------------------------------------------------------------
    // Config parsing
    // -------------------------------------------------------------------

    #[test]
    fn test_config_from_toml_full() {
        let toml_str = r#"
[federation]
enabled = true
node_name = "node-1"
bind_addr = "0.0.0.0:8092"

[federation.peers]
"node-2" = "192.168.1.102:8092"
"node-3" = "192.168.1.103:8092"

[federation.scheduling]
strategy = "packed"
"#;
        let config = FederationConfig::from_toml(toml_str).unwrap();
        assert!(config.enabled);
        assert_eq!(config.node_name, "node-1");
        assert_eq!(
            config.bind_addr,
            "0.0.0.0:8092".parse::<SocketAddr>().unwrap()
        );
        assert_eq!(config.peers.len(), 2);
        assert_eq!(config.scheduling_strategy, SchedulingStrategy::Packed);
    }

    #[test]
    fn test_config_from_toml_minimal() {
        let toml_str = r#"
[federation]
enabled = false
node_name = "solo"
bind_addr = "127.0.0.1:8092"
"#;
        let config = FederationConfig::from_toml(toml_str).unwrap();
        assert!(!config.enabled);
        assert_eq!(config.node_name, "solo");
        assert!(config.peers.is_empty());
        assert_eq!(config.scheduling_strategy, SchedulingStrategy::Balanced);
    }

    #[test]
    fn test_config_from_toml_invalid_addr() {
        let toml_str = r#"
[federation]
enabled = true
node_name = "bad"
bind_addr = "not-an-addr"
"#;
        assert!(FederationConfig::from_toml(toml_str).is_err());
    }

    #[test]
    fn test_config_from_toml_invalid_strategy() {
        let toml_str = r#"
[federation]
enabled = true
node_name = "bad"
bind_addr = "0.0.0.0:8092"

[federation.scheduling]
strategy = "yolo"
"#;
        assert!(FederationConfig::from_toml(toml_str).is_err());
    }

    #[test]
    fn test_config_default() {
        let config = FederationConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.scheduling_strategy, SchedulingStrategy::Balanced);
    }

    // -------------------------------------------------------------------
    // Scheduling strategy parsing
    // -------------------------------------------------------------------

    #[test]
    fn test_scheduling_strategy_from_str() {
        assert_eq!(
            "balanced".parse::<SchedulingStrategy>().unwrap(),
            SchedulingStrategy::Balanced
        );
        assert_eq!(
            "packed".parse::<SchedulingStrategy>().unwrap(),
            SchedulingStrategy::Packed
        );
        assert_eq!(
            "spread".parse::<SchedulingStrategy>().unwrap(),
            SchedulingStrategy::Spread
        );
        assert_eq!(
            "BALANCED".parse::<SchedulingStrategy>().unwrap(),
            SchedulingStrategy::Balanced
        );
        assert!("invalid".parse::<SchedulingStrategy>().is_err());
    }

    // -------------------------------------------------------------------
    // Stats
    // -------------------------------------------------------------------

    #[test]
    fn test_stats_single_node() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let cluster = FederationCluster::new(local);

        let stats = cluster.stats();
        assert_eq!(stats.total_nodes, 1);
        assert_eq!(stats.live_nodes, 1);
        assert_eq!(stats.suspect_nodes, 0);
        assert_eq!(stats.dead_nodes, 0);
        assert!(stats.coordinator_id.is_none());
    }

    #[test]
    fn test_stats_with_dead_node() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);

        let peer = make_node("node-2", "127.0.0.2:8092");
        let peer_id = peer.node_id.clone();
        cluster.register_node(peer).unwrap();

        let old_time = Utc::now() - Duration::seconds(60);
        cluster.set_heartbeat_time(&peer_id, old_time).unwrap();
        cluster.check_health();

        let stats = cluster.stats();
        assert_eq!(stats.total_nodes, 2);
        assert_eq!(stats.live_nodes, 1);
        assert_eq!(stats.dead_nodes, 1);
    }

    #[test]
    fn test_stats_with_coordinator() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let local_id = local.node_id.clone();
        let mut cluster = FederationCluster::new(local);

        cluster.start_election().unwrap();
        let stats = cluster.stats();
        assert_eq!(stats.coordinator_id, Some(local_id));
    }

    // -------------------------------------------------------------------
    // Edge cases
    // -------------------------------------------------------------------

    #[test]
    fn test_from_config() {
        let config = FederationConfig {
            enabled: true,
            node_name: "test-node".to_string(),
            bind_addr: "0.0.0.0:8092".parse().unwrap(),
            peers: HashMap::new(),
            scheduling_strategy: SchedulingStrategy::Spread,
        };
        let caps = NodeCapabilities {
            cpu_cores: 16,
            memory_mb: 65536,
            gpu_count: 4,
        };
        let cluster = FederationCluster::from_config(&config, caps);

        assert_eq!(cluster.node_count(), 1);
        let local = cluster.get_node(cluster.local_node_id()).unwrap();
        assert_eq!(local.name, "test-node");
        assert_eq!(local.capabilities.gpu_count, 4);
    }

    #[test]
    fn test_node_capabilities_default() {
        let caps = NodeCapabilities::default();
        assert_eq!(caps.cpu_cores, 4);
        assert_eq!(caps.memory_mb, 8192);
        assert_eq!(caps.gpu_count, 0);
    }

    #[test]
    fn test_become_coordinator_unknown_node_fails() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);
        assert!(cluster.become_coordinator("nonexistent").is_err());
    }

    #[test]
    fn test_display_traits() {
        assert_eq!(format!("{}", NodeRole::Coordinator), "coordinator");
        assert_eq!(format!("{}", NodeRole::Follower), "follower");
        assert_eq!(format!("{}", NodeRole::Candidate), "candidate");
        assert_eq!(format!("{}", NodeStatus::Online), "online");
        assert_eq!(format!("{}", NodeStatus::Suspect), "suspect");
        assert_eq!(format!("{}", NodeStatus::Dead), "dead");
        assert_eq!(format!("{}", SchedulingStrategy::Balanced), "balanced");
        assert_eq!(format!("{}", SchedulingStrategy::Packed), "packed");
        assert_eq!(format!("{}", SchedulingStrategy::Spread), "spread");
    }

    // ===================================================================
    // Federated Vector Store tests
    // ===================================================================

    #[test]
    fn test_federated_store_new() {
        let store =
            FederatedVectorStore::new("node-1".to_string(), VectorReplicationStrategy::Full);
        assert_eq!(store.local_node_id(), "node-1");
        assert_eq!(store.collection_count(), 0);
        assert!(store.collections().is_empty());
        assert_eq!(
            store.replication_strategy(),
            VectorReplicationStrategy::Full
        );
    }

    #[test]
    fn test_register_replica() {
        let mut store =
            FederatedVectorStore::new("node-1".to_string(), VectorReplicationStrategy::Full);
        let addr: SocketAddr = "10.0.0.2:8090".parse().unwrap();

        store.register_replica("embeddings", "node-2", addr, 100);
        assert_eq!(store.collection_count(), 1);
        assert_eq!(store.collections(), vec!["embeddings"]);

        let replicas = store.all_replicas("embeddings");
        assert_eq!(replicas.len(), 1);
        assert_eq!(replicas[0].node_id, "node-2");
        assert_eq!(replicas[0].vector_count, 100);
    }

    #[test]
    fn test_register_replica_updates_existing() {
        let mut store =
            FederatedVectorStore::new("node-1".to_string(), VectorReplicationStrategy::Full);
        let addr: SocketAddr = "10.0.0.2:8090".parse().unwrap();

        store.register_replica("col", "node-2", addr, 50);
        store.register_replica("col", "node-2", addr, 200);

        let replicas = store.all_replicas("col");
        assert_eq!(replicas.len(), 1);
        assert_eq!(replicas[0].vector_count, 200);
    }

    #[test]
    fn test_remote_replicas_excludes_local() {
        let mut store =
            FederatedVectorStore::new("node-1".to_string(), VectorReplicationStrategy::Full);
        let addr1: SocketAddr = "10.0.0.1:8090".parse().unwrap();
        let addr2: SocketAddr = "10.0.0.2:8090".parse().unwrap();

        store.register_replica("col", "node-1", addr1, 50);
        store.register_replica("col", "node-2", addr2, 50);

        let remote = store.remote_replicas("col");
        assert_eq!(remote.len(), 1);
        assert_eq!(remote[0].node_id, "node-2");
    }

    #[test]
    fn test_remove_node_from_replicas() {
        let mut store =
            FederatedVectorStore::new("node-1".to_string(), VectorReplicationStrategy::Full);
        let addr2: SocketAddr = "10.0.0.2:8090".parse().unwrap();
        let addr3: SocketAddr = "10.0.0.3:8090".parse().unwrap();

        store.register_replica("col-a", "node-2", addr2, 50);
        store.register_replica("col-a", "node-3", addr3, 30);
        store.register_replica("col-b", "node-2", addr2, 10);

        store.remove_node("node-2");

        assert_eq!(store.all_replicas("col-a").len(), 1);
        assert_eq!(store.all_replicas("col-a")[0].node_id, "node-3");
        assert_eq!(store.all_replicas("col-b").len(), 0);
    }

    #[test]
    fn test_insert_sync_messages() {
        let mut store =
            FederatedVectorStore::new("node-1".to_string(), VectorReplicationStrategy::Full);
        let addr2: SocketAddr = "10.0.0.2:8090".parse().unwrap();
        let addr3: SocketAddr = "10.0.0.3:8090".parse().unwrap();

        store.register_replica("col", "node-2", addr2, 0);
        store.register_replica("col", "node-3", addr3, 0);

        let entry = VectorSyncEntry {
            id: "vec-1".to_string(),
            embedding: vec![1.0, 2.0, 3.0],
            content: "hello".to_string(),
            metadata: serde_json::json!({}),
            created_at: Utc::now(),
        };

        let messages = store.insert_sync_messages("col", vec![entry]);
        assert_eq!(messages.len(), 2);
        assert!(messages.iter().any(|(addr, _)| *addr == addr2));
        assert!(messages.iter().any(|(addr, _)| *addr == addr3));
    }

    #[test]
    fn test_search_sync_messages() {
        let mut store =
            FederatedVectorStore::new("node-1".to_string(), VectorReplicationStrategy::Full);
        let addr2: SocketAddr = "10.0.0.2:8090".parse().unwrap();
        store.register_replica("col", "node-2", addr2, 100);

        let messages = store.search_sync_messages("col", &[1.0, 0.0], 10);
        assert_eq!(messages.len(), 1);

        match &messages[0].1 {
            VectorSyncMessage::Search {
                collection,
                query,
                top_k,
            } => {
                assert_eq!(collection, "col");
                assert_eq!(query, &[1.0, 0.0]);
                assert_eq!(*top_k, 10);
            }
            _ => panic!("Expected Search message"),
        }
    }

    #[test]
    fn test_search_sync_empty_for_unknown_collection() {
        let store =
            FederatedVectorStore::new("node-1".to_string(), VectorReplicationStrategy::Full);
        let messages = store.search_sync_messages("nonexistent", &[1.0], 5);
        assert!(messages.is_empty());
    }

    #[test]
    fn test_merge_results_deduplicates_and_ranks() {
        let store =
            FederatedVectorStore::new("node-1".to_string(), VectorReplicationStrategy::Full);

        let local = vec![
            RemoteSearchResult {
                id: "a".to_string(),
                score: 0.9,
                content: "doc a".to_string(),
                metadata: serde_json::json!({}),
                source_node: "node-1".to_string(),
            },
            RemoteSearchResult {
                id: "b".to_string(),
                score: 0.7,
                content: "doc b".to_string(),
                metadata: serde_json::json!({}),
                source_node: "node-1".to_string(),
            },
        ];

        let remote = vec![vec![
            RemoteSearchResult {
                id: "c".to_string(),
                score: 0.95,
                content: "doc c".to_string(),
                metadata: serde_json::json!({}),
                source_node: "node-2".to_string(),
            },
            // Duplicate of "a" with lower score — should be deduplicated.
            RemoteSearchResult {
                id: "a".to_string(),
                score: 0.85,
                content: "doc a".to_string(),
                metadata: serde_json::json!({}),
                source_node: "node-2".to_string(),
            },
        ]];

        let merged = store.merge_results(local, remote, 3);
        assert_eq!(merged.len(), 3);
        // Sorted by score: c(0.95), a(0.9), b(0.7).
        assert_eq!(merged[0].id, "c");
        assert_eq!(merged[1].id, "a");
        assert_eq!(merged[2].id, "b");
        // "a" should come from node-1 (higher score kept).
        assert_eq!(merged[1].source_node, "node-1");
    }

    #[test]
    fn test_merge_results_truncates_to_top_k() {
        let store =
            FederatedVectorStore::new("node-1".to_string(), VectorReplicationStrategy::Full);

        let local: Vec<RemoteSearchResult> = (0..10)
            .map(|i| RemoteSearchResult {
                id: format!("v{i}"),
                score: 1.0 - (i as f64 * 0.1),
                content: format!("doc {i}"),
                metadata: serde_json::json!({}),
                source_node: "node-1".to_string(),
            })
            .collect();

        let merged = store.merge_results(local, vec![], 3);
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].id, "v0");
    }

    #[test]
    fn test_announce_message() {
        let store =
            FederatedVectorStore::new("node-1".to_string(), VectorReplicationStrategy::Full);

        let msg = store.announce_message("embeddings", Some(768), 5000);
        match msg {
            VectorSyncMessage::AnnounceCollection {
                collection,
                dimension,
                vector_count,
            } => {
                assert_eq!(collection, "embeddings");
                assert_eq!(dimension, Some(768));
                assert_eq!(vector_count, 5000);
            }
            _ => panic!("Expected AnnounceCollection"),
        }
    }

    #[test]
    fn test_select_replica_nodes_full() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);
        let peer = make_node("node-2", "127.0.0.2:8092");
        cluster.register_node(peer).unwrap();

        let store =
            FederatedVectorStore::new("node-1".to_string(), VectorReplicationStrategy::Full);
        let nodes = store.select_replica_nodes(&cluster);
        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn test_select_replica_nodes_partial() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);
        let peer2 = make_node("node-2", "127.0.0.2:8092");
        let peer3 = make_node("node-3", "127.0.0.3:8092");
        cluster.register_node(peer2).unwrap();
        cluster.register_node(peer3).unwrap();

        let store = FederatedVectorStore::new(
            "node-1".to_string(),
            VectorReplicationStrategy::Partial {
                replication_factor: 2,
            },
        );
        let nodes = store.select_replica_nodes(&cluster);
        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn test_select_replica_nodes_sharded() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let local_id = local.node_id.clone();
        let mut cluster = FederationCluster::new(local);
        let peer = make_node("node-2", "127.0.0.2:8092");
        cluster.register_node(peer).unwrap();

        let store = FederatedVectorStore::new(local_id.clone(), VectorReplicationStrategy::Sharded);
        let nodes = store.select_replica_nodes(&cluster);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].node_id, local_id);
    }

    #[test]
    fn test_federated_stats() {
        let mut store =
            FederatedVectorStore::new("node-1".to_string(), VectorReplicationStrategy::Full);
        let addr2: SocketAddr = "10.0.0.2:8090".parse().unwrap();
        let addr3: SocketAddr = "10.0.0.3:8090".parse().unwrap();

        store.register_replica("col-a", "node-2", addr2, 100);
        store.register_replica("col-a", "node-3", addr3, 100);
        store.register_replica("col-b", "node-2", addr2, 50);

        let stats = store.stats();
        assert_eq!(stats.collection_count, 2);
        assert_eq!(stats.total_replicas, 3);
        assert_eq!(stats.total_vectors_across_replicas, 250);
        assert_eq!(stats.nodes_with_vectors, 2);
        assert_eq!(stats.replication_strategy, VectorReplicationStrategy::Full);
    }

    #[test]
    fn test_replication_strategy_default() {
        assert_eq!(
            VectorReplicationStrategy::default(),
            VectorReplicationStrategy::Full,
        );
    }

    #[test]
    fn test_vector_sync_message_serialization() {
        let msg = VectorSyncMessage::Search {
            collection: "test".to_string(),
            query: vec![1.0, 2.0],
            top_k: 5,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: VectorSyncMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            VectorSyncMessage::Search {
                collection,
                query,
                top_k,
            } => {
                assert_eq!(collection, "test");
                assert_eq!(query, vec![1.0, 2.0]);
                assert_eq!(top_k, 5);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_remote_replicas_empty_for_unknown() {
        let store =
            FederatedVectorStore::new("node-1".to_string(), VectorReplicationStrategy::Full);
        assert!(store.remote_replicas("nope").is_empty());
        assert!(store.all_replicas("nope").is_empty());
    }

    #[test]
    fn test_register_node_rejects_empty_id() {
        let addr: std::net::SocketAddr = "127.0.0.1:9000".parse().unwrap();
        let local = FederationNode::new("local".to_string(), addr, NodeCapabilities::default());
        let mut cluster = FederationCluster::new(local);
        let addr2: std::net::SocketAddr = "127.0.0.1:9001".parse().unwrap();
        let mut bad =
            FederationNode::new("badnode".to_string(), addr2, NodeCapabilities::default());
        bad.node_id = "".to_string();
        assert!(cluster.register_node(bad).is_err());
    }

    #[test]
    fn test_register_node_rejects_special_chars_in_id() {
        let addr: std::net::SocketAddr = "127.0.0.1:9000".parse().unwrap();
        let local = FederationNode::new("local".to_string(), addr, NodeCapabilities::default());
        let mut cluster = FederationCluster::new(local);
        let addr2: std::net::SocketAddr = "127.0.0.1:9001".parse().unwrap();
        let mut bad =
            FederationNode::new("badnode".to_string(), addr2, NodeCapabilities::default());
        bad.node_id = "node;rm -rf /".to_string();
        assert!(cluster.register_node(bad).is_err());
    }

    #[test]
    fn test_register_node_accepts_valid_id() {
        let addr: std::net::SocketAddr = "127.0.0.1:9000".parse().unwrap();
        let local = FederationNode::new("local".to_string(), addr, NodeCapabilities::default());
        let mut cluster = FederationCluster::new(local);
        let addr2: std::net::SocketAddr = "127.0.0.1:9001".parse().unwrap();
        let node =
            FederationNode::new("valid-node".to_string(), addr2, NodeCapabilities::default());
        assert!(cluster.register_node(node).is_ok());
    }
}
