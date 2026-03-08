//! Screen recording and streaming subsystem for the AGNOS desktop environment.
//!
//! Builds on top of [`screen_capture`](crate::screen_capture) to provide
//! frame-by-frame recording sessions that agents can poll for new frames.
//! Recording is synchronous — the caller drives frame capture in a loop
//! rather than relying on a background timer.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::RwLock;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::compositor::Compositor;
use crate::screen_capture::{CaptureError, CaptureFormat, CaptureTarget, ScreenCaptureManager};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Unique identifier for a recording session.
pub type RecordingId = Uuid;

/// Maximum number of frames retained in the ring buffer.
const MAX_RING_BUFFER_FRAMES: usize = 100;

/// Current state of a recording session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingState {
    /// Session created but not yet started (internal transition state).
    Idle,
    /// Actively recording frames.
    Recording,
    /// Recording is paused — frames cannot be captured until resumed.
    Paused,
    /// Recording has been stopped. No further frames can be captured.
    Stopped,
}

/// Configuration for a recording session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingConfig {
    /// What to record (full screen, window, or region).
    pub target: CaptureTarget,
    /// Minimum interval between frames in milliseconds.
    /// Default: 100 ms (10 fps) — suitable for agent consumption, not video.
    pub frame_interval_ms: u32,
    /// Maximum number of frames to capture before stopping.
    /// Default: 600 (1 minute at 10 fps).
    pub max_frames: Option<u32>,
    /// Maximum recording duration in seconds.
    /// Default: 60.
    pub max_duration_secs: Option<u64>,
    /// Output format for captured frames.
    pub format: CaptureFormat,
    /// Agent that requested the recording (if any).
    pub agent_id: Option<String>,
}

impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            target: CaptureTarget::FullScreen,
            frame_interval_ms: 100,
            max_frames: Some(600),
            max_duration_secs: Some(60),
            format: CaptureFormat::default(),
            agent_id: None,
        }
    }
}

/// A single captured frame within a recording session.
#[derive(Debug, Clone, Serialize)]
pub struct RecordedFrame {
    /// Zero-based sequence number within the session.
    pub sequence: u32,
    /// When this frame was captured.
    pub captured_at: DateTime<Utc>,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Encoded frame data (format determined by session config).
    #[serde(skip)]
    pub data: Vec<u8>,
    /// Size of the frame data in bytes.
    pub data_size: usize,
}

/// Public metadata for a recording session (no frame data).
#[derive(Debug, Clone, Serialize)]
pub struct RecordingSession {
    /// Unique session identifier.
    pub id: RecordingId,
    /// Session configuration.
    pub config: RecordingConfig,
    /// Current session state.
    pub state: RecordingState,
    /// When the session was started.
    pub started_at: DateTime<Utc>,
    /// Total number of frames captured so far.
    pub frame_count: u32,
    /// Total data size across all captured frames (including evicted ones).
    pub total_data_size: usize,
    /// Agent that owns this session (if any).
    pub agent_id: Option<String>,
}

/// Errors from the screen recording subsystem.
#[derive(Debug, Error)]
pub enum RecordingError {
    #[error("secure mode is active — screen recording is blocked")]
    SecureModeActive,
    #[error("agent '{0}' does not have screen capture permission")]
    PermissionDenied(String),
    #[error("recording session '{0}' not found")]
    SessionNotFound(RecordingId),
    #[error("agent already has an active recording session '{0}'")]
    AlreadyRecording(RecordingId),
    #[error("maximum frame count reached for this recording session")]
    MaxFramesReached,
    #[error("maximum recording duration reached for this recording session")]
    MaxDurationReached,
    #[error("capture error: {0}")]
    CaptureError(#[from] CaptureError),
    #[error("no frames have been captured yet")]
    NoFramesAvailable,
}

// ---------------------------------------------------------------------------
// Internal session state (includes frame buffer)
// ---------------------------------------------------------------------------

/// Internal representation of a recording session, including the frame buffer.
struct RecordingSessionInner {
    /// Public metadata.
    session: RecordingSession,
    /// Ring buffer of recent frames (max [`MAX_RING_BUFFER_FRAMES`]).
    frames: VecDeque<RecordedFrame>,
}

impl RecordingSessionInner {
    /// Convert to public metadata (no frames).
    fn to_public(&self) -> RecordingSession {
        self.session.clone()
    }
}

// ---------------------------------------------------------------------------
// ScreenRecordingManager
// ---------------------------------------------------------------------------

/// Manages screen recording sessions, enforcing one active recording per
/// agent and providing a polling-based streaming interface.
pub struct ScreenRecordingManager {
    /// All sessions (active and stopped, until explicitly removed).
    sessions: Arc<RwLock<HashMap<RecordingId, RecordingSessionInner>>>,
    /// Maps agent_id to their currently active recording (enforces 1 per agent).
    active_recordings: Arc<RwLock<HashMap<String, RecordingId>>>,
}

impl ScreenRecordingManager {
    /// Create a new recording manager with no active sessions.
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            active_recordings: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start a new recording session.
    ///
    /// - Checks secure mode via the compositor.
    /// - If an `agent_id` is provided, verifies the agent has capture permission.
    /// - Enforces at most one active recording per agent.
    pub fn start_recording(
        &self,
        compositor: &Compositor,
        capture_manager: &ScreenCaptureManager,
        config: RecordingConfig,
    ) -> Result<RecordingId, RecordingError> {
        // 1. Check secure mode
        let secure = *compositor
            .secure_mode
            .read()
            .unwrap_or_else(|e| e.into_inner());
        if secure {
            warn!("Screen recording blocked — secure mode active");
            return Err(RecordingError::SecureModeActive);
        }

        // 2. If agent_id provided, check permission
        if let Some(ref agent_id) = config.agent_id {
            let perm = capture_manager.get_permission(agent_id);
            if perm.is_none() {
                return Err(RecordingError::PermissionDenied(agent_id.clone()));
            }
        }

        // 3. Check one-recording-per-agent constraint
        if let Some(ref agent_id) = config.agent_id {
            let active = self
                .active_recordings
                .read()
                .unwrap_or_else(|e| e.into_inner());
            if let Some(&existing_id) = active.get(agent_id) {
                return Err(RecordingError::AlreadyRecording(existing_id));
            }
        }

        // 4. Create session
        let id = Uuid::new_v4();
        let now = Utc::now();
        let agent_id = config.agent_id.clone();

        let session = RecordingSession {
            id,
            config,
            state: RecordingState::Recording,
            started_at: now,
            frame_count: 0,
            total_data_size: 0,
            agent_id: agent_id.clone(),
        };

        let inner = RecordingSessionInner {
            session,
            frames: VecDeque::new(),
        };

        // 5. Register
        self.sessions
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, inner);

        if let Some(agent_id) = agent_id {
            self.active_recordings
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .insert(agent_id.clone(), id);
            info!(
                recording_id = %id,
                agent = %agent_id,
                "Screen recording started"
            );
        } else {
            info!(recording_id = %id, "System screen recording started");
        }

        Ok(id)
    }

    /// Capture a single frame for the given recording session.
    ///
    /// The caller drives the capture loop — this method is intentionally
    /// synchronous. Frames are stored in a ring buffer (max 100 retained).
    pub fn capture_frame(
        &self,
        compositor: &Compositor,
        capture_manager: &ScreenCaptureManager,
        recording_id: RecordingId,
    ) -> Result<RecordedFrame, RecordingError> {
        // 1. Validate session exists and is in Recording state
        let (target, format, agent_id) = {
            let sessions = self
                .sessions
                .read()
                .unwrap_or_else(|e| e.into_inner());
            let inner = sessions
                .get(&recording_id)
                .ok_or(RecordingError::SessionNotFound(recording_id))?;

            match inner.session.state {
                RecordingState::Recording => {}
                RecordingState::Paused => {
                    return Err(RecordingError::SessionNotFound(recording_id));
                }
                _ => {
                    return Err(RecordingError::SessionNotFound(recording_id));
                }
            }

            // 2. Check max_frames limit
            if let Some(max) = inner.session.config.max_frames {
                if inner.session.frame_count >= max {
                    return Err(RecordingError::MaxFramesReached);
                }
            }

            // 3. Check max_duration limit
            if let Some(max_secs) = inner.session.config.max_duration_secs {
                let elapsed = Utc::now()
                    .signed_duration_since(inner.session.started_at)
                    .num_seconds();
                if elapsed as u64 >= max_secs {
                    return Err(RecordingError::MaxDurationReached);
                }
            }

            (
                inner.session.config.target.clone(),
                inner.session.config.format,
                inner.session.config.agent_id.clone(),
            )
        };

        // 4. Use capture_manager to capture a frame
        let result = capture_manager.capture(
            compositor,
            target,
            format,
            agent_id.as_deref(),
        )?;

        // 5. Build RecordedFrame and store it
        let mut sessions = self
            .sessions
            .write()
            .unwrap_or_else(|e| e.into_inner());
        let inner = sessions
            .get_mut(&recording_id)
            .ok_or(RecordingError::SessionNotFound(recording_id))?;

        let frame = RecordedFrame {
            sequence: inner.session.frame_count,
            captured_at: result.captured_at,
            width: result.width,
            height: result.height,
            data_size: result.data_size,
            data: result.data,
        };

        // Update session metadata
        inner.session.frame_count += 1;
        inner.session.total_data_size += frame.data_size;

        // Ring buffer: evict oldest if at capacity
        if inner.frames.len() >= MAX_RING_BUFFER_FRAMES {
            inner.frames.pop_front();
        }
        inner.frames.push_back(frame.clone());

        debug!(
            recording_id = %recording_id,
            sequence = frame.sequence,
            width = frame.width,
            height = frame.height,
            "Frame captured"
        );

        Ok(frame)
    }

    /// Pause an active recording session.
    ///
    /// Transitions Recording -> Paused. Frames cannot be captured while paused.
    pub fn pause_recording(
        &self,
        recording_id: RecordingId,
    ) -> Result<(), RecordingError> {
        let mut sessions = self
            .sessions
            .write()
            .unwrap_or_else(|e| e.into_inner());
        let inner = sessions
            .get_mut(&recording_id)
            .ok_or(RecordingError::SessionNotFound(recording_id))?;

        if inner.session.state != RecordingState::Recording {
            return Err(RecordingError::SessionNotFound(recording_id));
        }

        inner.session.state = RecordingState::Paused;
        info!(recording_id = %recording_id, "Recording paused");
        Ok(())
    }

    /// Resume a paused recording session.
    ///
    /// Transitions Paused -> Recording.
    pub fn resume_recording(
        &self,
        recording_id: RecordingId,
    ) -> Result<(), RecordingError> {
        let mut sessions = self
            .sessions
            .write()
            .unwrap_or_else(|e| e.into_inner());
        let inner = sessions
            .get_mut(&recording_id)
            .ok_or(RecordingError::SessionNotFound(recording_id))?;

        if inner.session.state != RecordingState::Paused {
            return Err(RecordingError::SessionNotFound(recording_id));
        }

        inner.session.state = RecordingState::Recording;
        info!(recording_id = %recording_id, "Recording resumed");
        Ok(())
    }

    /// Stop a recording session and return its final metadata.
    ///
    /// The session is removed from the active recordings map. After stopping,
    /// the session data remains accessible via [`get_session`] and [`get_frames`]
    /// until the manager is dropped.
    pub fn stop_recording(
        &self,
        recording_id: RecordingId,
    ) -> Result<RecordingSession, RecordingError> {
        let mut sessions = self
            .sessions
            .write()
            .unwrap_or_else(|e| e.into_inner());
        let inner = sessions
            .get_mut(&recording_id)
            .ok_or(RecordingError::SessionNotFound(recording_id))?;

        if inner.session.state == RecordingState::Stopped {
            return Err(RecordingError::SessionNotFound(recording_id));
        }

        inner.session.state = RecordingState::Stopped;

        // Remove from active recordings
        if let Some(ref agent_id) = inner.session.agent_id {
            self.active_recordings
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .remove(agent_id);
        }

        info!(
            recording_id = %recording_id,
            frames = inner.session.frame_count,
            "Recording stopped"
        );

        Ok(inner.to_public())
    }

    /// Get session metadata (no frame data) for a specific recording.
    pub fn get_session(&self, recording_id: RecordingId) -> Option<RecordingSession> {
        self.sessions
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&recording_id)
            .map(|inner| inner.to_public())
    }

    /// List all recording sessions (active, paused, and stopped).
    pub fn list_sessions(&self) -> Vec<RecordingSession> {
        self.sessions
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .map(|inner| inner.to_public())
            .collect()
    }

    /// Get frames from a recording, optionally filtering to only those
    /// with sequence numbers greater than `since_sequence`.
    ///
    /// This is the primary streaming mechanism: agents poll periodically
    /// and pass the last sequence number they received to get only new frames.
    pub fn get_frames(
        &self,
        recording_id: RecordingId,
        since_sequence: Option<u32>,
    ) -> Result<Vec<RecordedFrame>, RecordingError> {
        let sessions = self
            .sessions
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let inner = sessions
            .get(&recording_id)
            .ok_or(RecordingError::SessionNotFound(recording_id))?;

        let frames: Vec<RecordedFrame> = match since_sequence {
            Some(seq) => inner
                .frames
                .iter()
                .filter(|f| f.sequence > seq)
                .cloned()
                .collect(),
            None => inner.frames.iter().cloned().collect(),
        };

        Ok(frames)
    }

    /// Get only the most recent frame from a recording (for live view).
    pub fn get_latest_frame(
        &self,
        recording_id: RecordingId,
    ) -> Result<RecordedFrame, RecordingError> {
        let sessions = self
            .sessions
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let inner = sessions
            .get(&recording_id)
            .ok_or(RecordingError::SessionNotFound(recording_id))?;

        inner
            .frames
            .back()
            .cloned()
            .ok_or(RecordingError::NoFramesAvailable)
    }
}

impl Default for ScreenRecordingManager {
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
    use crate::screen_capture::{
        CapturePermission, CaptureTargetKind, ScreenCaptureManager,
    };
    use crate::Compositor;

    fn setup() -> (Compositor, ScreenCaptureManager, ScreenRecordingManager) {
        let compositor = Compositor::with_resolution(800, 600);
        let capture_mgr = ScreenCaptureManager::new();
        let recording_mgr = ScreenRecordingManager::new();
        (compositor, capture_mgr, recording_mgr)
    }

    fn grant_full_access(manager: &ScreenCaptureManager, agent_id: &str) {
        manager.grant_permission(CapturePermission {
            agent_id: agent_id.to_string(),
            allowed_targets: vec![
                CaptureTargetKind::FullScreen,
                CaptureTargetKind::Window,
                CaptureTargetKind::Region,
            ],
            granted_at: Utc::now(),
            expires_at: None,
            max_captures_per_minute: 6000,
        });
    }

    fn default_config() -> RecordingConfig {
        RecordingConfig::default()
    }

    fn agent_config(agent_id: &str) -> RecordingConfig {
        RecordingConfig {
            agent_id: Some(agent_id.to_string()),
            ..Default::default()
        }
    }

    // -- 1. test_start_recording --

    #[test]
    fn test_start_recording() {
        let (compositor, capture_mgr, recording_mgr) = setup();
        let id = recording_mgr
            .start_recording(&compositor, &capture_mgr, default_config())
            .unwrap();
        let session = recording_mgr.get_session(id).unwrap();
        assert_eq!(session.state, RecordingState::Recording);
        assert_eq!(session.frame_count, 0);
    }

    // -- 2. test_start_recording_secure_mode --

    #[test]
    fn test_start_recording_secure_mode() {
        let (compositor, capture_mgr, recording_mgr) = setup();
        compositor.set_secure_mode(true);
        let err = recording_mgr
            .start_recording(&compositor, &capture_mgr, default_config())
            .unwrap_err();
        assert!(matches!(err, RecordingError::SecureModeActive));
    }

    // -- 3. test_start_recording_agent_no_permission --

    #[test]
    fn test_start_recording_agent_no_permission() {
        let (compositor, capture_mgr, recording_mgr) = setup();
        let err = recording_mgr
            .start_recording(&compositor, &capture_mgr, agent_config("rogue-agent"))
            .unwrap_err();
        assert!(matches!(err, RecordingError::PermissionDenied(_)));
    }

    // -- 4. test_start_recording_agent_with_permission --

    #[test]
    fn test_start_recording_agent_with_permission() {
        let (compositor, capture_mgr, recording_mgr) = setup();
        grant_full_access(&capture_mgr, "good-agent");
        let id = recording_mgr
            .start_recording(&compositor, &capture_mgr, agent_config("good-agent"))
            .unwrap();
        let session = recording_mgr.get_session(id).unwrap();
        assert_eq!(session.state, RecordingState::Recording);
        assert_eq!(session.agent_id.as_deref(), Some("good-agent"));
    }

    // -- 5. test_capture_frame --

    #[test]
    fn test_capture_frame() {
        let (compositor, capture_mgr, recording_mgr) = setup();
        let id = recording_mgr
            .start_recording(&compositor, &capture_mgr, default_config())
            .unwrap();
        let frame = recording_mgr
            .capture_frame(&compositor, &capture_mgr, id)
            .unwrap();
        assert_eq!(frame.sequence, 0);
        assert_eq!(frame.width, 800);
        assert_eq!(frame.height, 600);
        assert!(frame.data_size > 0);
    }

    // -- 6. test_capture_multiple_frames --

    #[test]
    fn test_capture_multiple_frames() {
        let (compositor, capture_mgr, recording_mgr) = setup();
        let id = recording_mgr
            .start_recording(&compositor, &capture_mgr, default_config())
            .unwrap();
        for i in 0..5 {
            let frame = recording_mgr
                .capture_frame(&compositor, &capture_mgr, id)
                .unwrap();
            assert_eq!(frame.sequence, i);
        }
        let session = recording_mgr.get_session(id).unwrap();
        assert_eq!(session.frame_count, 5);
    }

    // -- 7. test_capture_frame_session_not_found --

    #[test]
    fn test_capture_frame_session_not_found() {
        let (compositor, capture_mgr, recording_mgr) = setup();
        let fake_id = Uuid::new_v4();
        let err = recording_mgr
            .capture_frame(&compositor, &capture_mgr, fake_id)
            .unwrap_err();
        assert!(matches!(err, RecordingError::SessionNotFound(_)));
    }

    // -- 8. test_pause_and_resume --

    #[test]
    fn test_pause_and_resume() {
        let (compositor, capture_mgr, recording_mgr) = setup();
        let id = recording_mgr
            .start_recording(&compositor, &capture_mgr, default_config())
            .unwrap();

        // Capture a frame while recording
        recording_mgr
            .capture_frame(&compositor, &capture_mgr, id)
            .unwrap();

        // Pause
        recording_mgr.pause_recording(id).unwrap();
        let session = recording_mgr.get_session(id).unwrap();
        assert_eq!(session.state, RecordingState::Paused);

        // Capture while paused should fail
        let err = recording_mgr
            .capture_frame(&compositor, &capture_mgr, id)
            .unwrap_err();
        assert!(matches!(err, RecordingError::SessionNotFound(_)));

        // Resume
        recording_mgr.resume_recording(id).unwrap();
        let session = recording_mgr.get_session(id).unwrap();
        assert_eq!(session.state, RecordingState::Recording);

        // Capture should work again
        let frame = recording_mgr
            .capture_frame(&compositor, &capture_mgr, id)
            .unwrap();
        assert_eq!(frame.sequence, 1);
    }

    // -- 9. test_stop_recording --

    #[test]
    fn test_stop_recording() {
        let (compositor, capture_mgr, recording_mgr) = setup();
        let id = recording_mgr
            .start_recording(&compositor, &capture_mgr, default_config())
            .unwrap();
        recording_mgr
            .capture_frame(&compositor, &capture_mgr, id)
            .unwrap();
        recording_mgr
            .capture_frame(&compositor, &capture_mgr, id)
            .unwrap();

        let session = recording_mgr.stop_recording(id).unwrap();
        assert_eq!(session.state, RecordingState::Stopped);
        assert_eq!(session.frame_count, 2);
    }

    // -- 10. test_stop_already_stopped --

    #[test]
    fn test_stop_already_stopped() {
        let (compositor, capture_mgr, recording_mgr) = setup();
        let id = recording_mgr
            .start_recording(&compositor, &capture_mgr, default_config())
            .unwrap();
        recording_mgr.stop_recording(id).unwrap();

        let err = recording_mgr.stop_recording(id).unwrap_err();
        assert!(matches!(err, RecordingError::SessionNotFound(_)));
    }

    // -- 11. test_max_frames_limit --

    #[test]
    fn test_max_frames_limit() {
        let (compositor, capture_mgr, recording_mgr) = setup();
        let config = RecordingConfig {
            max_frames: Some(3),
            ..Default::default()
        };
        let id = recording_mgr
            .start_recording(&compositor, &capture_mgr, config)
            .unwrap();

        for _ in 0..3 {
            recording_mgr
                .capture_frame(&compositor, &capture_mgr, id)
                .unwrap();
        }

        let err = recording_mgr
            .capture_frame(&compositor, &capture_mgr, id)
            .unwrap_err();
        assert!(matches!(err, RecordingError::MaxFramesReached));
    }

    // -- 12. test_max_duration_limit --

    #[test]
    fn test_max_duration_limit() {
        let (compositor, capture_mgr, recording_mgr) = setup();
        let config = RecordingConfig {
            max_duration_secs: Some(0),
            ..Default::default()
        };
        let id = recording_mgr
            .start_recording(&compositor, &capture_mgr, config)
            .unwrap();

        let err = recording_mgr
            .capture_frame(&compositor, &capture_mgr, id)
            .unwrap_err();
        assert!(matches!(err, RecordingError::MaxDurationReached));
    }

    // -- 13. test_one_recording_per_agent --

    #[test]
    fn test_one_recording_per_agent() {
        let (compositor, capture_mgr, recording_mgr) = setup();
        grant_full_access(&capture_mgr, "agent-x");
        let id1 = recording_mgr
            .start_recording(&compositor, &capture_mgr, agent_config("agent-x"))
            .unwrap();

        let err = recording_mgr
            .start_recording(&compositor, &capture_mgr, agent_config("agent-x"))
            .unwrap_err();
        assert!(matches!(err, RecordingError::AlreadyRecording(rid) if rid == id1));
    }

    // -- 14. test_get_session --

    #[test]
    fn test_get_session() {
        let (compositor, capture_mgr, recording_mgr) = setup();
        let id = recording_mgr
            .start_recording(&compositor, &capture_mgr, default_config())
            .unwrap();
        let session = recording_mgr.get_session(id).unwrap();
        assert_eq!(session.id, id);
        assert_eq!(session.state, RecordingState::Recording);
        assert_eq!(session.frame_count, 0);

        // Non-existent session
        assert!(recording_mgr.get_session(Uuid::new_v4()).is_none());
    }

    // -- 15. test_list_sessions --

    #[test]
    fn test_list_sessions() {
        let (compositor, capture_mgr, recording_mgr) = setup();
        grant_full_access(&capture_mgr, "agent-a");
        grant_full_access(&capture_mgr, "agent-b");

        recording_mgr
            .start_recording(&compositor, &capture_mgr, default_config())
            .unwrap();
        recording_mgr
            .start_recording(&compositor, &capture_mgr, agent_config("agent-a"))
            .unwrap();
        recording_mgr
            .start_recording(&compositor, &capture_mgr, agent_config("agent-b"))
            .unwrap();

        let sessions = recording_mgr.list_sessions();
        assert_eq!(sessions.len(), 3);
    }

    // -- 16. test_get_frames_since --

    #[test]
    fn test_get_frames_since() {
        let (compositor, capture_mgr, recording_mgr) = setup();
        let id = recording_mgr
            .start_recording(&compositor, &capture_mgr, default_config())
            .unwrap();

        for _ in 0..5 {
            recording_mgr
                .capture_frame(&compositor, &capture_mgr, id)
                .unwrap();
        }

        let frames = recording_mgr.get_frames(id, Some(2)).unwrap();
        assert_eq!(frames.len(), 2); // sequences 3 and 4
        assert_eq!(frames[0].sequence, 3);
        assert_eq!(frames[1].sequence, 4);
    }

    // -- 17. test_get_frames_all --

    #[test]
    fn test_get_frames_all() {
        let (compositor, capture_mgr, recording_mgr) = setup();
        let id = recording_mgr
            .start_recording(&compositor, &capture_mgr, default_config())
            .unwrap();

        for _ in 0..5 {
            recording_mgr
                .capture_frame(&compositor, &capture_mgr, id)
                .unwrap();
        }

        let frames = recording_mgr.get_frames(id, None).unwrap();
        assert_eq!(frames.len(), 5);
    }

    // -- 18. test_get_latest_frame --

    #[test]
    fn test_get_latest_frame() {
        let (compositor, capture_mgr, recording_mgr) = setup();
        let id = recording_mgr
            .start_recording(&compositor, &capture_mgr, default_config())
            .unwrap();

        for _ in 0..3 {
            recording_mgr
                .capture_frame(&compositor, &capture_mgr, id)
                .unwrap();
        }

        let frame = recording_mgr.get_latest_frame(id).unwrap();
        assert_eq!(frame.sequence, 2);
    }

    // -- 19. test_get_latest_frame_empty --

    #[test]
    fn test_get_latest_frame_empty() {
        let (compositor, capture_mgr, recording_mgr) = setup();
        let id = recording_mgr
            .start_recording(&compositor, &capture_mgr, default_config())
            .unwrap();

        let err = recording_mgr.get_latest_frame(id).unwrap_err();
        assert!(matches!(err, RecordingError::NoFramesAvailable));
    }

    // -- 20. test_frame_ring_buffer --

    #[test]
    fn test_frame_ring_buffer() {
        let (compositor, capture_mgr, recording_mgr) = setup();
        let config = RecordingConfig {
            max_frames: Some(150),
            format: CaptureFormat::RawArgb,
            ..Default::default()
        };
        let id = recording_mgr
            .start_recording(&compositor, &capture_mgr, config)
            .unwrap();

        for _ in 0..110 {
            recording_mgr
                .capture_frame(&compositor, &capture_mgr, id)
                .unwrap();
        }

        // Only 100 frames retained in ring buffer
        let frames = recording_mgr.get_frames(id, None).unwrap();
        assert_eq!(frames.len(), MAX_RING_BUFFER_FRAMES);

        // Oldest retained frame should be sequence 10 (0-9 evicted)
        assert_eq!(frames[0].sequence, 10);
        assert_eq!(frames[99].sequence, 109);

        // But session metadata tracks all 110 frames
        let session = recording_mgr.get_session(id).unwrap();
        assert_eq!(session.frame_count, 110);
    }

    // -- 21. test_recording_default_config --

    #[test]
    fn test_recording_default_config() {
        let config = RecordingConfig::default();
        assert_eq!(config.frame_interval_ms, 100);
        assert_eq!(config.max_frames, Some(600));
        assert_eq!(config.max_duration_secs, Some(60));
        assert_eq!(config.format, CaptureFormat::Png);
        assert!(config.agent_id.is_none());
        assert!(matches!(config.target, CaptureTarget::FullScreen));
    }

    // -- 22. test_system_recording_no_agent --

    #[test]
    fn test_system_recording_no_agent() {
        let (compositor, capture_mgr, recording_mgr) = setup();
        // No agent_id means no permission check required
        let id = recording_mgr
            .start_recording(&compositor, &capture_mgr, default_config())
            .unwrap();
        let frame = recording_mgr
            .capture_frame(&compositor, &capture_mgr, id)
            .unwrap();
        assert_eq!(frame.sequence, 0);
        assert!(recording_mgr.get_session(id).unwrap().agent_id.is_none());
    }
}
