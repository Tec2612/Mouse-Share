use core_graphics::display::CGDisplay;
use ms_protocol::ScreenEdge;

const EDGE_MARGIN_PX: f64 = 1.0;

#[derive(Debug, Clone, Copy)]
pub struct VirtualScreenBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The union of every active display's bounds, in the same global
/// (top-left-origin, points-not-pixels) coordinate space `CGEventGetLocation`
/// reports in — macOS has no single "virtual screen" API the way Windows'
/// `SM_*VIRTUALSCREEN` metrics do, so this is assembled from
/// `CGGetActiveDisplayList` + `CGDisplayBounds` (via the `core-graphics`
/// crate's `CGDisplay::active_displays`/`bounds`).
pub fn virtual_screen_bounds() -> VirtualScreenBounds {
    let displays = CGDisplay::active_displays().unwrap_or_default();
    if displays.is_empty() {
        // Should be unreachable on a running system (there is always at
        // least the main display), but a degenerate zero-size rect is a
        // safe fallback that simply never reports an edge.
        return VirtualScreenBounds { x: 0.0, y: 0.0, width: 0.0, height: 0.0 };
    }

    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    for id in displays {
        let bounds = CGDisplay::new(id).bounds();
        min_x = min_x.min(bounds.origin.x);
        min_y = min_y.min(bounds.origin.y);
        max_x = max_x.max(bounds.origin.x + bounds.size.width);
        max_y = max_y.max(bounds.origin.y + bounds.size.height);
    }

    VirtualScreenBounds { x: min_x, y: min_y, width: max_x - min_x, height: max_y - min_y }
}

pub fn detect_edge(x: f64, y: f64, bounds: VirtualScreenBounds) -> Option<(ScreenEdge, f64)> {
    let right = bounds.x + bounds.width;
    let bottom = bounds.y + bounds.height;

    let normalized_y = ((y - bounds.y) / bounds.height.max(1.0)).clamp(0.0, 1.0);
    let normalized_x = ((x - bounds.x) / bounds.width.max(1.0)).clamp(0.0, 1.0);

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
        VirtualScreenBounds { x: 0.0, y: 0.0, width: 1920.0, height: 1080.0 }
    }

    #[test]
    fn left_and_right_edges_are_detected() {
        assert_eq!(detect_edge(0.0, 540.0, bounds()).unwrap().0, ScreenEdge::Left);
        assert_eq!(detect_edge(1920.0, 540.0, bounds()).unwrap().0, ScreenEdge::Right);
    }

    #[test]
    fn interior_point_touches_no_edge() {
        assert!(detect_edge(960.0, 540.0, bounds()).is_none());
    }
}
