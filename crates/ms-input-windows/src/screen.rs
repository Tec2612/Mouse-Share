use ms_protocol::ScreenEdge;
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

/// A pixel of slack at the boundary: hooks report discrete pixel
/// positions, and demanding the literal `x == 0` can miss a fast swipe
/// that jumps straight past it between samples.
const EDGE_MARGIN_PX: i32 = 1;

#[derive(Debug, Clone, Copy)]
pub struct VirtualScreenBounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Reads the bounding rectangle of the entire multi-monitor desktop (not
/// any single display), via `GetSystemMetrics(SM_*VIRTUALSCREEN)`. This is
/// what a compound arrangement's edges are measured against: the
/// left/right/top/bottom the user configures in the screen layout is the
/// outer edge of this whole rectangle, not of an individual monitor,
/// which matters when this machine itself has more than one display.
///
/// Requires the process to have called
/// `SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)` at startup (done
/// once in `ms-daemon::main`); otherwise these values — and the cursor
/// positions compared against them — are DPI-virtualized rather than
/// physical pixels, silently misplacing the boundary on non-100%-scale
/// displays.
pub fn virtual_screen_bounds() -> VirtualScreenBounds {
    // SAFETY: GetSystemMetrics is safe to call with any of the documented
    // SM_* indices; no pointers or lifetimes are involved.
    unsafe {
        VirtualScreenBounds {
            x: GetSystemMetrics(SM_XVIRTUALSCREEN),
            y: GetSystemMetrics(SM_YVIRTUALSCREEN),
            width: GetSystemMetrics(SM_CXVIRTUALSCREEN),
            height: GetSystemMetrics(SM_CYVIRTUALSCREEN),
        }
    }
}

/// Returns which configured edge (if any) `(x, y)` is touching, and the
/// normalized 0.0..1.0 position along that edge, given the current
/// virtual screen bounds.
pub fn detect_edge(x: i32, y: i32, bounds: VirtualScreenBounds) -> Option<(ScreenEdge, f64)> {
    let right = bounds.x + bounds.width - 1;
    let bottom = bounds.y + bounds.height - 1;

    let normalized_y = ((y - bounds.y) as f64 / (bounds.height - 1).max(1) as f64).clamp(0.0, 1.0);
    let normalized_x = ((x - bounds.x) as f64 / (bounds.width - 1).max(1) as f64).clamp(0.0, 1.0);

    if x <= bounds.x + EDGE_MARGIN_PX {
        Some((ScreenEdge::Left, normalized_y))
    } else if x >= right - EDGE_MARGIN_PX {
        Some((ScreenEdge::Right, normalized_y))
    } else if y <= bounds.y + EDGE_MARGIN_PX {
        Some((ScreenEdge::Top, normalized_x))
    } else if y >= bottom - EDGE_MARGIN_PX {
        Some((ScreenEdge::Bottom, normalized_x))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> VirtualScreenBounds {
        VirtualScreenBounds { x: 0, y: 0, width: 1920, height: 1080 }
    }

    #[test]
    fn left_edge_is_detected_with_correct_normalized_y() {
        let (edge, pos) = detect_edge(0, 540, bounds()).unwrap();
        assert_eq!(edge, ScreenEdge::Left);
        assert!((pos - 0.5).abs() < 0.01);
    }

    #[test]
    fn right_edge_is_detected() {
        let (edge, _) = detect_edge(1919, 100, bounds()).unwrap();
        assert_eq!(edge, ScreenEdge::Right);
    }

    #[test]
    fn top_and_bottom_edges_are_detected_with_normalized_x() {
        let (edge, pos) = detect_edge(960, 0, bounds()).unwrap();
        assert_eq!(edge, ScreenEdge::Top);
        assert!((pos - 0.5).abs() < 0.01);

        let (edge, _) = detect_edge(960, 1079, bounds()).unwrap();
        assert_eq!(edge, ScreenEdge::Bottom);
    }

    #[test]
    fn interior_point_touches_no_edge() {
        assert!(detect_edge(960, 540, bounds()).is_none());
    }

    #[test]
    fn a_secondary_monitor_offset_to_the_left_shifts_the_left_edge_accordingly() {
        // Multi-monitor: virtual desktop origin can be negative when a
        // display is arranged to the left of the primary.
        let offset_bounds = VirtualScreenBounds { x: -1920, y: 0, width: 3840, height: 1080 };
        let (edge, _) = detect_edge(-1920, 200, offset_bounds).unwrap();
        assert_eq!(edge, ScreenEdge::Left);
        assert!(detect_edge(0, 200, offset_bounds).is_none(), "the seam between two monitors isn't a virtual-desktop edge");
    }
}
