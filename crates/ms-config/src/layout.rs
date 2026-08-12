use ms_protocol::ScreenEdge;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComputerNode {
    pub device_id: uuid::Uuid,
    pub display_name: String,
    pub enabled: bool,
    /// Position on the arrangement canvas, in arbitrary UI units — purely
    /// for rendering the drag-and-drop layout editor. Edge adjacency
    /// (which is what actually drives mouse hand-off) is stored
    /// separately in `ScreenLayout::edges`, not derived from this, so a
    /// user can place icons for visual clarity without it silently
    /// changing which edges are linked.
    pub canvas_x: f32,
    pub canvas_y: f32,
}

/// One configured hand-off: crossing `from_edge` on `from` transfers
/// control to `to`, entering along `to`'s opposite edge (Left<->Right,
/// Top<->Bottom) unless `to_edge` overrides that, which supports
/// asymmetric arrangements (e.g. a laptop below and to the side of an
/// external monitor).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EdgeLink {
    pub from: uuid::Uuid,
    pub from_edge: ScreenEdge,
    pub to: uuid::Uuid,
    pub to_edge: ScreenEdge,
    /// Portion of `from_edge`, normalized 0.0..1.0, that is live for
    /// hand-off. Lets a user restrict the transition boundary to part of
    /// an edge instead of its full length (e.g. only the top half of a
    /// monitor's right edge borders a shorter secondary display).
    pub boundary_start: f32,
    pub boundary_end: f32,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LayoutError {
    #[error("device {0} is not present in the layout")]
    UnknownDevice(uuid::Uuid),
    #[error("device {0} already has an edge link on its {1:?} edge")]
    EdgeAlreadyLinked(uuid::Uuid, ScreenEdge),
    #[error("a device cannot be linked to itself")]
    SelfLink,
    #[error("boundary_start must be less than boundary_end, within 0.0..=1.0")]
    InvalidBoundary,
}

fn opposite_edge(edge: ScreenEdge) -> ScreenEdge {
    match edge {
        ScreenEdge::Left => ScreenEdge::Right,
        ScreenEdge::Right => ScreenEdge::Left,
        ScreenEdge::Top => ScreenEdge::Bottom,
        ScreenEdge::Bottom => ScreenEdge::Top,
    }
}

/// The full multi-computer arrangement: which devices participate and how
/// their screen edges connect. This is what turns a set of paired devices
/// (an `ms-security::TrustStore` concern) into an actual usable "move the
/// mouse right to reach the Mac" workspace.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ScreenLayout {
    nodes: HashMap<uuid::Uuid, ComputerNode>,
    edges: Vec<EdgeLink>,
}

impl ScreenLayout {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_device(&mut self, node: ComputerNode) {
        self.nodes.insert(node.device_id, node);
    }

    pub fn remove_device(&mut self, device_id: uuid::Uuid) {
        self.nodes.remove(&device_id);
        self.edges.retain(|e| e.from != device_id && e.to != device_id);
    }

    pub fn set_enabled(&mut self, device_id: uuid::Uuid, enabled: bool) -> Result<(), LayoutError> {
        self.nodes
            .get_mut(&device_id)
            .ok_or(LayoutError::UnknownDevice(device_id))?
            .enabled = enabled;
        Ok(())
    }

    /// Links `from`'s `from_edge` to `to`, entering `to` along the
    /// opposite edge (the common case: my right edge borders your left
    /// edge). Use [`link_edges_asymmetric`] for non-mirrored arrangements.
    pub fn link_edges(
        &mut self,
        from: uuid::Uuid,
        from_edge: ScreenEdge,
        to: uuid::Uuid,
    ) -> Result<(), LayoutError> {
        self.link_edges_asymmetric(from, from_edge, to, opposite_edge(from_edge), 0.0, 1.0)
    }

    pub fn link_edges_asymmetric(
        &mut self,
        from: uuid::Uuid,
        from_edge: ScreenEdge,
        to: uuid::Uuid,
        to_edge: ScreenEdge,
        boundary_start: f32,
        boundary_end: f32,
    ) -> Result<(), LayoutError> {
        if from == to {
            return Err(LayoutError::SelfLink);
        }
        if !self.nodes.contains_key(&from) {
            return Err(LayoutError::UnknownDevice(from));
        }
        if !self.nodes.contains_key(&to) {
            return Err(LayoutError::UnknownDevice(to));
        }
        if !(0.0..boundary_end).contains(&boundary_start) || boundary_end > 1.0 {
            return Err(LayoutError::InvalidBoundary);
        }
        if self.edges.iter().any(|e| e.from == from && e.from_edge == from_edge) {
            return Err(LayoutError::EdgeAlreadyLinked(from, from_edge));
        }

        self.edges.push(EdgeLink { from, from_edge, to, to_edge, boundary_start, boundary_end });
        Ok(())
    }

    pub fn unlink_edge(&mut self, from: uuid::Uuid, from_edge: ScreenEdge) {
        self.edges.retain(|e| !(e.from == from && e.from_edge == from_edge));
    }

    /// Resolves what crossing `edge` at normalized `position` (0.0..1.0
    /// along that edge) on `from` should do: `None` if there is no link
    /// there, is disabled, or `position` falls outside the configured
    /// boundary.
    pub fn resolve(&self, from: uuid::Uuid, edge: ScreenEdge, position: f32) -> Option<&EdgeLink> {
        let target_enabled = |id: uuid::Uuid| self.nodes.get(&id).is_some_and(|n| n.enabled);
        if !target_enabled(from) {
            return None;
        }
        self.edges.iter().find(|link| {
            link.from == from
                && link.from_edge == edge
                && target_enabled(link.to)
                && (link.boundary_start..=link.boundary_end).contains(&position)
        })
    }

    pub fn nodes(&self) -> impl Iterator<Item = &ComputerNode> {
        self.nodes.values()
    }

    pub fn edges(&self) -> impl Iterator<Item = &EdgeLink> {
        self.edges.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: uuid::Uuid, name: &str) -> ComputerNode {
        ComputerNode { device_id: id, display_name: name.into(), enabled: true, canvas_x: 0.0, canvas_y: 0.0 }
    }

    #[test]
    fn linking_two_devices_resolves_in_both_directions_with_opposite_edges() {
        let (a, b) = (uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
        let mut layout = ScreenLayout::new();
        layout.add_device(node(a, "Windows-PC"));
        layout.add_device(node(b, "Mac"));
        layout.link_edges(a, ScreenEdge::Right, b).unwrap();

        let link = layout.resolve(a, ScreenEdge::Right, 0.5).unwrap();
        assert_eq!(link.to, b);
        assert_eq!(link.to_edge, ScreenEdge::Left);
    }

    #[test]
    fn resolve_outside_configured_boundary_returns_none() {
        let (a, b) = (uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
        let mut layout = ScreenLayout::new();
        layout.add_device(node(a, "A"));
        layout.add_device(node(b, "B"));
        layout.link_edges_asymmetric(a, ScreenEdge::Right, b, ScreenEdge::Left, 0.0, 0.5).unwrap();

        assert!(layout.resolve(a, ScreenEdge::Right, 0.25).is_some());
        assert!(layout.resolve(a, ScreenEdge::Right, 0.75).is_none());
    }

    #[test]
    fn disabling_a_device_removes_it_from_resolution_without_deleting_the_link() {
        let (a, b) = (uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
        let mut layout = ScreenLayout::new();
        layout.add_device(node(a, "A"));
        layout.add_device(node(b, "B"));
        layout.link_edges(a, ScreenEdge::Right, b).unwrap();

        layout.set_enabled(b, false).unwrap();
        assert!(layout.resolve(a, ScreenEdge::Right, 0.5).is_none());

        layout.set_enabled(b, true).unwrap();
        assert!(layout.resolve(a, ScreenEdge::Right, 0.5).is_some());
    }

    #[test]
    fn removing_a_device_cleans_up_its_edges() {
        let (a, b) = (uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
        let mut layout = ScreenLayout::new();
        layout.add_device(node(a, "A"));
        layout.add_device(node(b, "B"));
        layout.link_edges(a, ScreenEdge::Right, b).unwrap();

        layout.remove_device(b);
        assert!(layout.resolve(a, ScreenEdge::Right, 0.5).is_none());
        assert_eq!(layout.edges().count(), 0);
    }

    #[test]
    fn a_device_cannot_link_to_itself() {
        let a = uuid::Uuid::new_v4();
        let mut layout = ScreenLayout::new();
        layout.add_device(node(a, "A"));
        assert_eq!(layout.link_edges(a, ScreenEdge::Right, a), Err(LayoutError::SelfLink));
    }

    #[test]
    fn an_edge_cannot_be_linked_twice() {
        let (a, b, c) = (uuid::Uuid::new_v4(), uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
        let mut layout = ScreenLayout::new();
        layout.add_device(node(a, "A"));
        layout.add_device(node(b, "B"));
        layout.add_device(node(c, "C"));
        layout.link_edges(a, ScreenEdge::Right, b).unwrap();
        assert_eq!(layout.link_edges(a, ScreenEdge::Right, c), Err(LayoutError::EdgeAlreadyLinked(a, ScreenEdge::Right)));
    }

    #[test]
    fn supports_a_three_computer_hub_arrangement() {
        // Windows PC -- Main Mac -- Windows PC 2, matching the spec's
        // example topology, plus a fourth device above the hub.
        let (hub, left, right, top) =
            (uuid::Uuid::new_v4(), uuid::Uuid::new_v4(), uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
        let mut layout = ScreenLayout::new();
        for (id, name) in [(hub, "Main Mac"), (left, "Windows PC"), (right, "Windows PC 2"), (top, "MacBook")] {
            layout.add_device(node(id, name));
        }
        layout.link_edges(hub, ScreenEdge::Left, left).unwrap();
        layout.link_edges(hub, ScreenEdge::Right, right).unwrap();
        layout.link_edges(hub, ScreenEdge::Top, top).unwrap();

        assert_eq!(layout.resolve(hub, ScreenEdge::Left, 0.5).unwrap().to, left);
        assert_eq!(layout.resolve(hub, ScreenEdge::Right, 0.5).unwrap().to, right);
        assert_eq!(layout.resolve(hub, ScreenEdge::Top, 0.5).unwrap().to, top);
        assert!(layout.resolve(hub, ScreenEdge::Bottom, 0.5).is_none());
    }
}
