//! Detects a mouse "shake" — rapid back-and-forth motion — like macOS' shake-to-find-cursor.
//!
//! Pure logic: the caller feeds pointer positions with timestamps; both axes are tracked so a
//! horizontal, vertical or diagonal shake all count.

/// Minimum length of a stroke, in pixels, for its end to count as a reversal.
const MIN_STROKE: i32 = 30;
/// All reversals of one shake must happen within this many milliseconds.
const WINDOW_MS: u32 = 600;
/// Reversals needed to call it a shake.
const REVERSALS: usize = 4;

#[derive(Debug, Default)]
pub struct ShakeDetector {
    x: Axis,
    y: Axis,
}

impl ShakeDetector {
    pub const fn new() -> Self {
        Self {
            x: Axis::new(),
            y: Axis::new(),
        }
    }

    /// Feeds a pointer position and its timestamp (milliseconds, any monotonic origin).
    /// Returns `true` once per detected shake; the detector then starts over.
    pub fn feed(&mut self, x: i32, y: i32, time: u32) -> bool {
        let shaken_x = self.x.feed(x, time);
        let shaken_y = self.y.feed(y, time);
        if shaken_x || shaken_y {
            self.x.reset();
            self.y.reset();
            return true;
        }
        false
    }
}

/// Reversal tracking along one axis.
#[derive(Debug, Default)]
struct Axis {
    last: Option<i32>,
    /// Sign of the current stroke's direction (0 before the first move).
    direction: i32,
    /// Distance travelled in the current direction.
    stroke: i32,
    /// Timestamps of recent qualifying reversals.
    reversals: Vec<u32>,
}

impl Axis {
    const fn new() -> Self {
        Self {
            last: None,
            direction: 0,
            stroke: 0,
            reversals: Vec::new(),
        }
    }

    fn feed(&mut self, pos: i32, time: u32) -> bool {
        let Some(last) = self.last.replace(pos) else {
            return false;
        };
        let delta = pos - last;
        if delta == 0 {
            return false;
        }
        let direction = delta.signum();
        if direction == self.direction {
            self.stroke += delta.abs();
            return false;
        }
        // Direction changed: the stroke that just ended decides whether this is a reversal.
        let qualifies = self.stroke >= MIN_STROKE;
        self.direction = direction;
        self.stroke = delta.abs();
        if !qualifies {
            return false;
        }
        self.reversals.push(time);
        self.reversals
            .retain(|&t| time.wrapping_sub(t) <= WINDOW_MS);
        self.reversals.len() >= REVERSALS
    }

    fn reset(&mut self) {
        self.reversals.clear();
        self.stroke = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feeds `strokes` alternating left/right strokes of `length` px, each taking `stroke_ms`.
    fn shake(detector: &mut ShakeDetector, strokes: usize, length: i32, stroke_ms: u32) -> bool {
        let (mut x, mut t, mut detected) = (500, 1000, false);
        for i in 0..strokes {
            let dir = if i % 2 == 0 { 1 } else { -1 };
            for _ in 0..5 {
                x += dir * length / 5;
                t += stroke_ms / 5;
                detected |= detector.feed(x, 300, t);
            }
        }
        detected
    }

    #[test]
    fn fast_wide_shake_is_detected() {
        assert!(shake(&mut ShakeDetector::default(), 6, 60, 80));
    }

    #[test]
    fn vertical_shake_is_detected_too() {
        let mut d = ShakeDetector::default();
        let (mut y, mut t, mut hit) = (400, 0, false);
        for i in 0..6 {
            let dir = if i % 2 == 0 { 1 } else { -1 };
            for _ in 0..4 {
                y += dir * 15;
                t += 20;
                hit |= d.feed(300, y, t);
            }
        }
        assert!(hit);
    }

    #[test]
    fn slow_oscillation_is_ignored() {
        assert!(!shake(&mut ShakeDetector::default(), 6, 60, 400));
    }

    #[test]
    fn small_jitter_is_ignored() {
        assert!(!shake(&mut ShakeDetector::default(), 12, 10, 40));
    }

    #[test]
    fn straight_motion_is_ignored() {
        let mut d = ShakeDetector::default();
        assert!((0..100).all(|i| !d.feed(i * 10, 200, i as u32 * 10)));
    }

    #[test]
    fn fires_once_then_needs_a_new_shake() {
        let mut d = ShakeDetector::default();
        assert!(shake(&mut d, 6, 60, 80));
        // Continuing straight after the detection must not re-trigger.
        assert!(!(0..10).any(|i| d.feed(2000 + i * 20, 300, 5000 + i as u32 * 10)));
        assert!(shake(&mut d, 6, 60, 80));
    }
}
