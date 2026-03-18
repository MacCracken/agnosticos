//! Multi-touch gesture recognition for the AGNOS desktop environment.
//!
//! Recognises taps, double-taps, long-presses, swipes, pinch-to-zoom,
//! and rotation gestures from raw touch events.

use serde::{Deserialize, Serialize};

use crate::compositor::SurfaceId;

// ============================================================================
// Touch primitives
// ============================================================================

/// A single touch contact point.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TouchPoint {
    pub id: u32,
    pub x: f64,
    pub y: f64,
    pub pressure: f64,
    pub timestamp_ms: u64,
}

/// Direction of a swipe gesture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SwipeDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Recognised gesture type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GestureType {
    Tap,
    DoubleTap,
    LongPress,
    Swipe(SwipeDirection),
    Pinch { scale: f64 },
    Rotate { angle: f64 },
    Pan { dx: f64, dy: f64 },
}

/// A fully recognised gesture event.
#[derive(Debug, Clone)]
pub struct GestureEvent {
    pub gesture: GestureType,
    pub touches: Vec<TouchPoint>,
    pub surface_id: Option<SurfaceId>,
    pub timestamp_ms: u64,
}

// ============================================================================
// Configuration
// ============================================================================

/// Tuning parameters for gesture recognition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GestureConfig {
    pub tap_timeout_ms: u64,
    pub double_tap_interval_ms: u64,
    pub long_press_ms: u64,
    pub swipe_threshold_px: f64,
    pub pinch_threshold: f64,
}

impl Default for GestureConfig {
    fn default() -> Self {
        Self {
            tap_timeout_ms: 300,
            double_tap_interval_ms: 400,
            long_press_ms: 500,
            swipe_threshold_px: 50.0,
            pinch_threshold: 0.1,
        }
    }
}

// ============================================================================
// Internal tracking
// ============================================================================

/// Tracks a single finger from touch_down to touch_up.
#[derive(Debug, Clone)]
struct TouchTracker {
    start: TouchPoint,
    current: TouchPoint,
    released: bool,
    release_time: Option<u64>,
}

// ============================================================================
// Gesture recogniser
// ============================================================================

/// Stateful multi-touch gesture recogniser.
///
/// Feed raw touch events via `touch_down`, `touch_move`, `touch_up` and then
/// drain recognised gestures with `recognized_gestures`.
#[derive(Debug)]
pub struct GestureRecognizer {
    config: GestureConfig,
    active_touches: Vec<TouchTracker>,
    recognised: Vec<GestureEvent>,
    last_tap_time: Option<u64>,
    last_tap_position: Option<(f64, f64)>,
}

impl GestureRecognizer {
    /// Create a recogniser with the given configuration.
    pub fn new(config: GestureConfig) -> Self {
        Self {
            config,
            active_touches: Vec::new(),
            recognised: Vec::new(),
            last_tap_time: None,
            last_tap_position: None,
        }
    }

    /// Register a new touch contact.
    pub fn touch_down(&mut self, point: TouchPoint) {
        self.active_touches.push(TouchTracker {
            start: point,
            current: point,
            released: false,
            release_time: None,
        });
    }

    /// Update the position of an existing touch contact.
    pub fn touch_move(&mut self, point: TouchPoint) {
        if let Some(tracker) = self
            .active_touches
            .iter_mut()
            .find(|t| t.start.id == point.id)
        {
            tracker.current = point;
        }

        // Check for two-finger gestures while moving.
        self.check_two_finger_gestures();
    }

    /// Lift a finger. This triggers single-finger gesture recognition.
    pub fn touch_up(&mut self, id: u32) {
        let now = self
            .active_touches
            .iter()
            .find(|t| t.start.id == id)
            .map(|t| t.current.timestamp_ms)
            .unwrap_or(0);

        if let Some(tracker) = self.active_touches.iter_mut().find(|t| t.start.id == id) {
            tracker.released = true;
            tracker.release_time = Some(now);
        }

        // Only process single-finger gestures when exactly one finger was involved.
        let active_count = self.active_touches.len();
        let all_released = self.active_touches.iter().all(|t| t.released);

        if active_count == 1 && all_released {
            self.recognise_single_finger();
            self.active_touches.clear();
        } else if all_released {
            self.active_touches.clear();
        }
    }

    /// Drain and return all recognised gestures since the last call.
    pub fn recognized_gestures(&mut self) -> Vec<GestureEvent> {
        std::mem::take(&mut self.recognised)
    }

    /// Number of fingers currently touching.
    pub fn active_touch_count(&self) -> usize {
        self.active_touches.iter().filter(|t| !t.released).count()
    }

    /// Reset all state.
    pub fn reset(&mut self) {
        self.active_touches.clear();
        self.recognised.clear();
        self.last_tap_time = None;
        self.last_tap_position = None;
    }

    // -- private helpers --

    fn recognise_single_finger(&mut self) {
        let tracker = match self.active_touches.first() {
            Some(t) => t.clone(),
            None => return,
        };

        let dx = tracker.current.x - tracker.start.x;
        let dy = tracker.current.y - tracker.start.y;
        let distance = (dx * dx + dy * dy).sqrt();
        let duration = tracker
            .release_time
            .unwrap_or(tracker.current.timestamp_ms)
            .saturating_sub(tracker.start.timestamp_ms);

        if distance > self.config.swipe_threshold_px {
            // Swipe
            let direction = if dx.abs() > dy.abs() {
                if dx > 0.0 {
                    SwipeDirection::Right
                } else {
                    SwipeDirection::Left
                }
            } else if dy > 0.0 {
                SwipeDirection::Down
            } else {
                SwipeDirection::Up
            };
            self.recognised.push(GestureEvent {
                gesture: GestureType::Swipe(direction),
                touches: vec![tracker.start, tracker.current],
                surface_id: None,
                timestamp_ms: tracker.current.timestamp_ms,
            });
        } else if duration >= self.config.long_press_ms {
            // Long press
            self.recognised.push(GestureEvent {
                gesture: GestureType::LongPress,
                touches: vec![tracker.start],
                surface_id: None,
                timestamp_ms: tracker.current.timestamp_ms,
            });
        } else if duration < self.config.tap_timeout_ms {
            // Tap (possibly double-tap)
            let is_double = if let (Some(last_time), Some(last_pos)) =
                (self.last_tap_time, self.last_tap_position)
            {
                let interval = tracker.start.timestamp_ms.saturating_sub(last_time);
                let tap_dx = tracker.start.x - last_pos.0;
                let tap_dy = tracker.start.y - last_pos.1;
                let tap_dist = (tap_dx * tap_dx + tap_dy * tap_dy).sqrt();
                interval <= self.config.double_tap_interval_ms
                    && tap_dist < self.config.swipe_threshold_px
            } else {
                false
            };

            if is_double {
                self.recognised.push(GestureEvent {
                    gesture: GestureType::DoubleTap,
                    touches: vec![tracker.start],
                    surface_id: None,
                    timestamp_ms: tracker.current.timestamp_ms,
                });
                self.last_tap_time = None;
                self.last_tap_position = None;
            } else {
                self.recognised.push(GestureEvent {
                    gesture: GestureType::Tap,
                    touches: vec![tracker.start],
                    surface_id: None,
                    timestamp_ms: tracker.current.timestamp_ms,
                });
                self.last_tap_time = Some(tracker.current.timestamp_ms);
                self.last_tap_position = Some((tracker.start.x, tracker.start.y));
            }
        }
    }

    fn check_two_finger_gestures(&mut self) {
        if self.active_touches.len() != 2 {
            return;
        }
        let t0 = &self.active_touches[0];
        let t1 = &self.active_touches[1];

        if t0.released || t1.released {
            return;
        }

        // Initial distance
        let start_dx = t0.start.x - t1.start.x;
        let start_dy = t0.start.y - t1.start.y;
        let start_dist = (start_dx * start_dx + start_dy * start_dy).sqrt();

        // Current distance
        let cur_dx = t0.current.x - t1.current.x;
        let cur_dy = t0.current.y - t1.current.y;
        let cur_dist = (cur_dx * cur_dx + cur_dy * cur_dy).sqrt();

        if start_dist > 0.0 {
            let scale = cur_dist / start_dist;
            if (scale - 1.0).abs() > self.config.pinch_threshold {
                // Pinch detected
                let touches = vec![t0.current, t1.current];
                let ts = t0.current.timestamp_ms.max(t1.current.timestamp_ms);
                self.recognised.push(GestureEvent {
                    gesture: GestureType::Pinch { scale },
                    touches,
                    surface_id: None,
                    timestamp_ms: ts,
                });
            }
        }

        // Rotation: angle change between the two fingers
        let start_angle = start_dy.atan2(start_dx);
        let cur_angle = cur_dy.atan2(cur_dx);
        let angle_diff = cur_angle - start_angle;
        if angle_diff.abs() > 0.1 {
            let touches = vec![t0.current, t1.current];
            let ts = t0.current.timestamp_ms.max(t1.current.timestamp_ms);
            self.recognised.push(GestureEvent {
                gesture: GestureType::Rotate { angle: angle_diff },
                touches,
                surface_id: None,
                timestamp_ms: ts,
            });
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> GestureConfig {
        GestureConfig {
            tap_timeout_ms: 300,
            double_tap_interval_ms: 400,
            long_press_ms: 500,
            swipe_threshold_px: 50.0,
            pinch_threshold: 0.1,
        }
    }

    fn point(id: u32, x: f64, y: f64, ts: u64) -> TouchPoint {
        TouchPoint {
            id,
            x,
            y,
            pressure: 1.0,
            timestamp_ms: ts,
        }
    }

    #[test]
    fn test_recognizer_new() {
        let r = GestureRecognizer::new(config());
        assert_eq!(r.active_touch_count(), 0);
    }

    #[test]
    fn test_tap() {
        let mut r = GestureRecognizer::new(config());
        r.touch_down(point(1, 100.0, 100.0, 0));
        assert_eq!(r.active_touch_count(), 1);
        r.touch_up(1);
        let gestures = r.recognized_gestures();
        assert_eq!(gestures.len(), 1);
        assert!(matches!(gestures[0].gesture, GestureType::Tap));
    }

    #[test]
    fn test_double_tap() {
        let mut r = GestureRecognizer::new(config());
        // First tap
        r.touch_down(point(1, 100.0, 100.0, 0));
        r.touch_up(1);
        let g1 = r.recognized_gestures();
        assert_eq!(g1.len(), 1);
        assert!(matches!(g1[0].gesture, GestureType::Tap));

        // Second tap within interval at same position
        r.touch_down(point(2, 100.0, 100.0, 200));
        r.touch_up(2);
        let g2 = r.recognized_gestures();
        assert_eq!(g2.len(), 1);
        assert!(matches!(g2[0].gesture, GestureType::DoubleTap));
    }

    #[test]
    fn test_long_press() {
        let mut r = GestureRecognizer::new(config());
        r.touch_down(point(1, 100.0, 100.0, 0));
        // Simulate time passing without moving
        let p = point(1, 100.0, 100.0, 600);
        r.touch_move(p);
        r.touch_up(1);
        let gestures = r.recognized_gestures();
        assert_eq!(gestures.len(), 1);
        assert!(matches!(gestures[0].gesture, GestureType::LongPress));
    }

    #[test]
    fn test_swipe_right() {
        let mut r = GestureRecognizer::new(config());
        r.touch_down(point(1, 100.0, 100.0, 0));
        r.touch_move(point(1, 200.0, 105.0, 100));
        r.touch_up(1);
        let gestures = r.recognized_gestures();
        assert_eq!(gestures.len(), 1);
        match &gestures[0].gesture {
            GestureType::Swipe(d) => assert_eq!(*d, SwipeDirection::Right),
            other => panic!("Expected Swipe(Right), got {:?}", other),
        }
    }

    #[test]
    fn test_swipe_left() {
        let mut r = GestureRecognizer::new(config());
        r.touch_down(point(1, 200.0, 100.0, 0));
        r.touch_move(point(1, 50.0, 105.0, 100));
        r.touch_up(1);
        let gestures = r.recognized_gestures();
        assert!(matches!(
            gestures[0].gesture,
            GestureType::Swipe(SwipeDirection::Left)
        ));
    }

    #[test]
    fn test_swipe_up() {
        let mut r = GestureRecognizer::new(config());
        r.touch_down(point(1, 100.0, 200.0, 0));
        r.touch_move(point(1, 105.0, 50.0, 100));
        r.touch_up(1);
        let gestures = r.recognized_gestures();
        assert!(matches!(
            gestures[0].gesture,
            GestureType::Swipe(SwipeDirection::Up)
        ));
    }

    #[test]
    fn test_swipe_down() {
        let mut r = GestureRecognizer::new(config());
        r.touch_down(point(1, 100.0, 50.0, 0));
        r.touch_move(point(1, 105.0, 200.0, 100));
        r.touch_up(1);
        let gestures = r.recognized_gestures();
        assert!(matches!(
            gestures[0].gesture,
            GestureType::Swipe(SwipeDirection::Down)
        ));
    }

    #[test]
    fn test_pinch() {
        let mut r = GestureRecognizer::new(config());
        // Two fingers starting close, moving apart
        r.touch_down(point(1, 100.0, 100.0, 0));
        r.touch_down(point(2, 110.0, 100.0, 0));
        // Move apart
        r.touch_move(point(1, 50.0, 100.0, 100));
        r.touch_move(point(2, 160.0, 100.0, 100));
        let gestures = r.recognized_gestures();
        let pinch = gestures
            .iter()
            .find(|g| matches!(g.gesture, GestureType::Pinch { .. }));
        assert!(pinch.is_some());
        if let GestureType::Pinch { scale } = &pinch.unwrap().gesture {
            assert!(*scale > 1.0); // fingers moved apart
        }
    }

    #[test]
    fn test_rotate() {
        let mut r = GestureRecognizer::new(config());
        // Two fingers, one above the other, then rotate
        r.touch_down(point(1, 100.0, 100.0, 0));
        r.touch_down(point(2, 100.0, 200.0, 0));
        // Rotate: move finger 1 right, finger 2 left
        r.touch_move(point(1, 150.0, 100.0, 100));
        r.touch_move(point(2, 50.0, 200.0, 100));
        let gestures = r.recognized_gestures();
        let rotate = gestures
            .iter()
            .find(|g| matches!(g.gesture, GestureType::Rotate { .. }));
        assert!(rotate.is_some());
    }

    #[test]
    fn test_reset() {
        let mut r = GestureRecognizer::new(config());
        r.touch_down(point(1, 100.0, 100.0, 0));
        r.reset();
        assert_eq!(r.active_touch_count(), 0);
        assert!(r.recognized_gestures().is_empty());
    }

    #[test]
    fn test_active_touch_count() {
        let mut r = GestureRecognizer::new(config());
        assert_eq!(r.active_touch_count(), 0);
        r.touch_down(point(1, 0.0, 0.0, 0));
        assert_eq!(r.active_touch_count(), 1);
        r.touch_down(point(2, 10.0, 10.0, 0));
        assert_eq!(r.active_touch_count(), 2);
    }

    #[test]
    fn test_gesture_config_default() {
        let cfg = GestureConfig::default();
        assert_eq!(cfg.tap_timeout_ms, 300);
        assert_eq!(cfg.double_tap_interval_ms, 400);
        assert_eq!(cfg.long_press_ms, 500);
        assert!((cfg.swipe_threshold_px - 50.0).abs() < f64::EPSILON);
        assert!((cfg.pinch_threshold - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn test_recognized_gestures_drains() {
        let mut r = GestureRecognizer::new(config());
        r.touch_down(point(1, 100.0, 100.0, 0));
        r.touch_up(1);
        let g1 = r.recognized_gestures();
        assert!(!g1.is_empty());
        let g2 = r.recognized_gestures();
        assert!(g2.is_empty());
    }

    #[test]
    fn test_double_tap_too_slow() {
        let mut r = GestureRecognizer::new(config());
        // First tap
        r.touch_down(point(1, 100.0, 100.0, 0));
        r.touch_up(1);
        r.recognized_gestures(); // drain

        // Second tap after interval expires
        r.touch_down(point(2, 100.0, 100.0, 1000));
        r.touch_up(2);
        let g = r.recognized_gestures();
        assert_eq!(g.len(), 1);
        assert!(matches!(g[0].gesture, GestureType::Tap)); // Not DoubleTap
    }

    #[test]
    fn test_swipe_direction_variants() {
        let dirs = [
            SwipeDirection::Up,
            SwipeDirection::Down,
            SwipeDirection::Left,
            SwipeDirection::Right,
        ];
        assert_eq!(dirs.len(), 4);
    }

    #[test]
    fn test_touch_point_fields() {
        let p = TouchPoint {
            id: 42,
            x: 1.5,
            y: 2.5,
            pressure: 0.8,
            timestamp_ms: 12345,
        };
        assert_eq!(p.id, 42);
        assert!((p.pressure - 0.8).abs() < f64::EPSILON);
    }
}
