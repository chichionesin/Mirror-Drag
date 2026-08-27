//! Pure window geometry: center-anchored ("symmetric") resizing and centering.
//!
//! No platform types live here, so the logic is unit-tested on any host.

/// Screen rectangle in physical pixels; `right`/`bottom` are exclusive (Win32 `RECT` semantics).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub const fn width(&self) -> i32 {
        self.right - self.left
    }

    pub const fn height(&self) -> i32 {
        self.bottom - self.top
    }

    /// The same rectangle shifted by `(dx, dy)`.
    pub const fn offset(&self, dx: i32, dy: i32) -> Self {
        Self::new(
            self.left + dx,
            self.top + dy,
            self.right + dx,
            self.bottom + dy,
        )
    }
}

/// Which window edges the user grabbed. A corner sets one horizontal and one vertical flag.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Edges {
    pub left: bool,
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
}

/// Size constraints of a window, in pixels (mirrors `WM_GETMINMAXINFO` track sizes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub min_width: i32,
    pub min_height: i32,
    pub max_width: i32,
    pub max_height: i32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            min_width: 0,
            min_height: 0,
            max_width: i32::MAX / 2,
            max_height: i32::MAX / 2,
        }
    }
}

/// Resizes `start` so the dragged edge follows the cursor delta while the opposite edge
/// mirrors it, keeping the window center fixed — macOS Option+drag behaviour.
///
/// The result honours `limits`; when a limit is hit the window stops growing/shrinking
/// but stays centered on the same point.
pub fn symmetric_resize(start: Rect, edges: Edges, dx: i32, dy: i32, limits: &Limits) -> Rect {
    let (left, right) = resize_axis(
        (start.left, start.right),
        (edges.left, edges.right),
        dx,
        (limits.min_width, limits.max_width),
    );
    let (top, bottom) = resize_axis(
        (start.top, start.bottom),
        (edges.top, edges.bottom),
        dy,
        (limits.min_height, limits.max_height),
    );
    Rect {
        left,
        top,
        right,
        bottom,
    }
}

/// One axis of [`symmetric_resize`]: both ends move outward by the same amount.
fn resize_axis(
    (lo, hi): (i32, i32),
    (drag_lo, drag_hi): (bool, bool),
    delta: i32,
    (min, max): (i32, i32),
) -> (i32, i32) {
    // Growth applied to *each* end. Pulling the high edge outward is a positive delta,
    // pulling the low edge outward is a negative one.
    let grow = match (drag_lo, drag_hi) {
        (false, false) => return (lo, hi),
        (true, _) => -delta,
        (false, true) => delta,
    };
    let size = hi - lo;
    // The total size changes by 2*grow; keep it inside [min, max] using ceil/floor of the
    // half-differences so an odd limit is never violated.
    let min_grow = (min - size + 1).div_euclid(2);
    let max_grow = (max - size).div_euclid(2).max(min_grow);
    let grow = grow.clamp(min_grow, max_grow);
    (lo - grow, hi + grow)
}

/// Places `rect` at the center of `area`, keeping its size. A rectangle larger than the
/// area is aligned to the area's top/left edge so its title bar stays reachable.
pub fn center_in(rect: Rect, area: Rect) -> Rect {
    let (w, h) = (rect.width(), rect.height());
    let left = if w > area.width() {
        area.left
    } else {
        area.left + (area.width() - w) / 2
    };
    let top = if h > area.height() {
        area.top
    } else {
        area.top + (area.height() - h) / 2
    };
    Rect::new(left, top, left + w, top + h)
}

#[cfg(test)]
mod tests {
    use super::*;

    const START: Rect = Rect::new(100, 100, 300, 250); // 200 x 150
    const RIGHT: Edges = Edges {
        right: true,
        ..Edges::none()
    };
    const LEFT: Edges = Edges {
        left: true,
        ..Edges::none()
    };
    const TOP_LEFT: Edges = Edges {
        top: true,
        left: true,
        ..Edges::none()
    };

    impl Edges {
        const fn none() -> Self {
            Self {
                left: false,
                top: false,
                right: false,
                bottom: false,
            }
        }
    }

    fn free() -> Limits {
        Limits::default()
    }

    #[test]
    fn dragging_right_edge_mirrors_left_edge() {
        let r = symmetric_resize(START, RIGHT, 10, 999, &free());
        assert_eq!(r, Rect::new(90, 100, 310, 250));
    }

    #[test]
    fn dragging_left_edge_outward_grows_both_sides() {
        let r = symmetric_resize(START, LEFT, -10, 0, &free());
        assert_eq!(r, Rect::new(90, 100, 310, 250));
    }

    #[test]
    fn dragging_left_edge_inward_shrinks_both_sides() {
        let r = symmetric_resize(START, LEFT, 10, 0, &free());
        assert_eq!(r, Rect::new(110, 100, 290, 250));
    }

    #[test]
    fn corner_drag_resizes_both_axes() {
        let r = symmetric_resize(START, TOP_LEFT, -10, -20, &free());
        assert_eq!(r, Rect::new(90, 80, 310, 270));
    }

    #[test]
    fn no_edges_leaves_rect_untouched() {
        assert_eq!(
            symmetric_resize(START, Edges::default(), 50, 50, &free()),
            START
        );
    }

    #[test]
    fn min_size_is_respected_and_center_kept() {
        let limits = Limits {
            min_width: 191,
            ..free()
        };
        let r = symmetric_resize(START, RIGHT, -50, 0, &limits);
        assert_eq!(r, Rect::new(104, 100, 296, 250)); // 192 wide, never below 191
        assert_eq!(r.left + r.right, START.left + START.right);
    }

    #[test]
    fn max_size_is_respected() {
        let limits = Limits {
            max_width: 211,
            ..free()
        };
        let r = symmetric_resize(START, RIGHT, 50, 0, &limits);
        assert_eq!(r, Rect::new(95, 100, 305, 250)); // 210 wide, never above 211
    }

    #[test]
    fn inconsistent_limits_do_not_panic() {
        let limits = Limits {
            min_width: 500,
            max_width: 100,
            ..free()
        };
        let r = symmetric_resize(START, RIGHT, 1, 0, &limits);
        assert_eq!(r.width(), 500);
    }

    #[test]
    fn centers_in_work_area() {
        let area = Rect::new(0, 0, 1920, 1040);
        assert_eq!(
            center_in(Rect::new(5, 5, 805, 605), area),
            Rect::new(560, 220, 1360, 820)
        );
    }

    #[test]
    fn centers_on_monitor_with_negative_origin() {
        let area = Rect::new(-1920, -200, 0, 880);
        assert_eq!(
            center_in(Rect::new(0, 0, 400, 300), area),
            Rect::new(-1160, 190, -760, 490)
        );
    }

    #[test]
    fn oversized_window_aligns_to_top_left() {
        let area = Rect::new(0, 0, 800, 600);
        assert_eq!(
            center_in(Rect::new(0, 0, 1000, 700), area),
            Rect::new(0, 0, 1000, 700)
        );
    }
}
