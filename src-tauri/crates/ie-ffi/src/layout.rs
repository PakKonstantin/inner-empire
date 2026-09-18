//! Laying a graph out.
//!
//! A force simulation is the kind of code that is subtly wrong in ways you
//! only notice by looking at it — nodes drifting off screen, a cluster
//! collapsing to a point, the whole thing never settling. Since there is no
//! way to look at it from here, it is written where it can be asserted on
//! instead: does it converge, do linked notes end up closer than unlinked
//! ones, is it the same every time.
//!
//! Deterministic on purpose. A graph that rearranges itself each time it opens
//! is a graph nobody builds a mental map of, and a non-deterministic layout
//! cannot be tested at all.

use std::collections::HashMap;

use crate::types::{GraphData, GraphNode};

/// A node's place on the canvas.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct NodePosition {
    pub id: String,
    pub x: f32,
    pub y: f32,
    /// Radius, from the node's degree. Carried here so the view does not have
    /// to invent a scale that then differs between platforms.
    pub radius: f32,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct GraphLayout {
    pub positions: Vec<NodePosition>,
    /// The bounding box, so the view can fit the graph without measuring it.
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
    /// How many passes it took to settle, or the cap if it did not.
    pub iterations: u32,
    pub settled: bool,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct LayoutOptions {
    /// Cap on passes. A phone should not spend a second on a graph the user is
    /// about to pan away from.
    #[uniffi(default = 300)]
    pub max_iterations: u32,
    /// Stop early once nothing is moving much.
    #[uniffi(default = 0.01)]
    pub settle_threshold: f32,
    /// Roughly how far apart unconnected nodes want to be.
    #[uniffi(default = 60.0)]
    pub spacing: f32,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            max_iterations: 300,
            settle_threshold: 0.01,
            spacing: 60.0,
        }
    }
}

/// Position a graph's nodes.
///
/// Fruchterman–Reingold: repulsion between every pair, attraction along every
/// edge, and a temperature that cools so the thing stops rather than
/// oscillating. Chosen over anything cleverer because it is short enough to
/// read and its behaviour is easy to state, which is what makes it testable.
#[uniffi::export]
pub fn layout_graph(graph: GraphData, options: LayoutOptions) -> GraphLayout {
    let count = graph.nodes.len();
    if count == 0 {
        return GraphLayout {
            positions: Vec::new(),
            min_x: 0.0,
            min_y: 0.0,
            max_x: 0.0,
            max_y: 0.0,
            iterations: 0,
            settled: true,
        };
    }

    let area = options.spacing * options.spacing * count as f32;
    let ideal = options.spacing.max(1.0);
    let width = area.sqrt();

    // Deterministic starting points on a spiral rather than at random: the
    // same vault lays out the same way every time it is opened, which is what
    // lets someone remember where things are.
    let mut xs: Vec<f32> = Vec::with_capacity(count);
    let mut ys: Vec<f32> = Vec::with_capacity(count);
    for i in 0..count {
        let angle = i as f32 * 2.399_963; // the golden angle, in radians
        let radius = width * 0.5 * ((i as f32 + 0.5) / count as f32).sqrt();
        xs.push(radius * angle.cos());
        ys.push(radius * angle.sin());
    }

    let index: HashMap<&str, usize> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| (node.id.as_str(), i))
        .collect();

    let edges: Vec<(usize, usize)> = graph
        .edges
        .iter()
        .filter_map(|edge| {
            let from = *index.get(edge.source.as_str())?;
            let to = *index.get(edge.target.as_str())?;
            (from != to).then_some((from, to))
        })
        .collect();

    let mut temperature = width * 0.1;
    let cooling = 0.95_f32;
    let mut iterations = 0u32;
    let mut settled = false;

    let mut dx = vec![0.0f32; count];
    let mut dy = vec![0.0f32; count];

    for pass in 0..options.max_iterations {
        iterations = pass + 1;
        dx.iter_mut().for_each(|v| *v = 0.0);
        dy.iter_mut().for_each(|v| *v = 0.0);

        // Repulsion. O(n²), which is fine at the sizes a phone will draw and
        // is why `max_nodes` exists on the query side.
        for i in 0..count {
            for j in (i + 1)..count {
                let (mut ox, mut oy) = (xs[i] - xs[j], ys[i] - ys[j]);
                let mut distance = (ox * ox + oy * oy).sqrt();
                if distance < 0.01 {
                    // Two nodes exactly on top of each other have no direction
                    // to push apart in. Nudge deterministically rather than
                    // randomly, or the layout stops being reproducible.
                    ox = ((i % 7) as f32 - 3.0) * 0.01;
                    oy = ((j % 7) as f32 - 3.0) * 0.01;
                    distance = (ox * ox + oy * oy).sqrt().max(0.001);
                }
                let force = ideal * ideal / distance;
                let (ux, uy) = (ox / distance, oy / distance);
                dx[i] += ux * force;
                dy[i] += uy * force;
                dx[j] -= ux * force;
                dy[j] -= uy * force;
            }
        }

        // Attraction along edges.
        for &(from, to) in &edges {
            let (ox, oy) = (xs[from] - xs[to], ys[from] - ys[to]);
            let distance = (ox * ox + oy * oy).sqrt().max(0.001);
            let force = distance * distance / ideal;
            let (ux, uy) = (ox / distance, oy / distance);
            dx[from] -= ux * force;
            dy[from] -= uy * force;
            dx[to] += ux * force;
            dy[to] += uy * force;
        }

        let mut movement = 0.0f32;
        for i in 0..count {
            let length = (dx[i] * dx[i] + dy[i] * dy[i]).sqrt();
            if length < 1e-6 {
                continue;
            }
            // Capped by the temperature, which is what stops a node shooting
            // across the canvas on one badly-conditioned pass.
            let step = length.min(temperature);
            xs[i] += dx[i] / length * step;
            ys[i] += dy[i] / length * step;
            movement += step;
        }

        temperature *= cooling;
        if (movement / count as f32) < options.settle_threshold {
            settled = true;
            break;
        }
    }

    let mut positions = Vec::with_capacity(count);
    let (mut min_x, mut min_y) = (f32::MAX, f32::MAX);
    let (mut max_x, mut max_y) = (f32::MIN, f32::MIN);
    for (i, node) in graph.nodes.iter().enumerate() {
        let radius = radius_for(node);
        min_x = min_x.min(xs[i] - radius);
        min_y = min_y.min(ys[i] - radius);
        max_x = max_x.max(xs[i] + radius);
        max_y = max_y.max(ys[i] + radius);
        positions.push(NodePosition {
            id: node.id.clone(),
            x: xs[i],
            y: ys[i],
            radius,
        });
    }

    GraphLayout {
        positions,
        min_x,
        min_y,
        max_x,
        max_y,
        iterations,
        settled,
    }
}

/// A node's size, from how connected it is.
///
/// Square-rooted so the difference between one link and four is visible while
/// the difference between forty and eighty is not overwhelming — degree has a
/// long tail, and a linear scale makes one hub swamp the view.
fn radius_for(node: &GraphNode) -> f32 {
    let degree = node.degree as f32;
    (4.0 + degree.sqrt() * 2.0).min(24.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{GraphEdge, LinkKind};

    fn node(id: &str, degree: u32) -> GraphNode {
        GraphNode {
            id: id.to_string(),
            path: None,
            label: id.to_string(),
            kind: crate::types::GraphNodeKind::Note,
            degree,
            tags: Vec::new(),
            folder: String::new(),
        }
    }

    fn edge(from: &str, to: &str) -> GraphEdge {
        GraphEdge {
            source: from.to_string(),
            target: to.to_string(),
            kind: LinkKind::WikiLink,
        }
    }

    fn graph(nodes: Vec<GraphNode>, edges: Vec<GraphEdge>) -> GraphData {
        GraphData {
            nodes,
            edges,
            truncated: false,
        }
    }

    fn distance(layout: &GraphLayout, a: &str, b: &str) -> f32 {
        let find = |id: &str| layout.positions.iter().find(|p| p.id == id).unwrap();
        let (a, b) = (find(a), find(b));
        ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
    }

    #[test]
    fn an_empty_graph_lays_out_to_nothing() {
        let layout = layout_graph(graph(Vec::new(), Vec::new()), LayoutOptions::default());
        assert!(layout.positions.is_empty());
        assert!(layout.settled);
    }

    #[test]
    fn a_single_node_does_not_wander() {
        let layout = layout_graph(
            graph(vec![node("a", 0)], Vec::new()),
            LayoutOptions::default(),
        );
        assert_eq!(layout.positions.len(), 1);
        assert!(layout.positions[0].x.is_finite());
        assert!(layout.positions[0].y.is_finite());
    }

    #[test]
    fn linked_notes_end_up_closer_than_unlinked_ones() {
        // The whole point of the view. If this does not hold, the graph is
        // decoration rather than information.
        let nodes = vec![node("a", 1), node("b", 1), node("c", 0), node("d", 0)];
        let layout = layout_graph(graph(nodes, vec![edge("a", "b")]), LayoutOptions::default());

        let linked = distance(&layout, "a", "b");
        let unlinked = distance(&layout, "c", "d");
        assert!(
            linked < unlinked,
            "linked pair {linked} should be closer than unlinked pair {unlinked}"
        );
    }

    #[test]
    fn a_hub_sits_between_the_notes_that_link_to_it() {
        let nodes = vec![node("hub", 3), node("a", 1), node("b", 1), node("c", 1)];
        let edges = vec![edge("a", "hub"), edge("b", "hub"), edge("c", "hub")];
        let layout = layout_graph(graph(nodes, edges), LayoutOptions::default());

        // Each spoke is nearer the hub than any other spoke.
        for (spoke, other) in [("a", "b"), ("b", "c"), ("c", "a")] {
            assert!(
                distance(&layout, spoke, "hub") < distance(&layout, spoke, other),
                "{spoke} should sit nearer the hub than {other}"
            );
        }
    }

    #[test]
    fn the_same_graph_lays_out_the_same_way_every_time() {
        // A graph that rearranges itself on each open is one nobody builds a
        // mental map of — and a non-deterministic layout cannot be tested.
        let build = || {
            graph(
                vec![node("a", 2), node("b", 1), node("c", 1)],
                vec![edge("a", "b"), edge("a", "c")],
            )
        };
        let first = layout_graph(build(), LayoutOptions::default());
        let second = layout_graph(build(), LayoutOptions::default());
        assert_eq!(first.positions, second.positions);
        assert_eq!(first.iterations, second.iterations);
    }

    #[test]
    fn nodes_stacked_on_one_spot_are_pushed_apart() {
        // Every node starting at the same place has no direction to separate
        // in; a naive implementation divides by zero and produces NaN, and the
        // whole view goes blank.
        let nodes: Vec<GraphNode> = (0..6).map(|i| node(&format!("n{i}"), 0)).collect();
        let layout = layout_graph(graph(nodes, Vec::new()), LayoutOptions::default());

        for position in &layout.positions {
            assert!(
                position.x.is_finite() && position.y.is_finite(),
                "{position:?}"
            );
        }
        for i in 0..layout.positions.len() {
            for j in (i + 1)..layout.positions.len() {
                let d = distance(&layout, &format!("n{i}"), &format!("n{j}"));
                assert!(d > 1.0, "n{i} and n{j} are on top of each other");
            }
        }
    }

    #[test]
    fn it_settles_rather_than_running_to_the_cap() {
        let nodes: Vec<GraphNode> = (0..20).map(|i| node(&format!("n{i}"), 1)).collect();
        let edges: Vec<GraphEdge> = (0..19)
            .map(|i| edge(&format!("n{i}"), &format!("n{}", i + 1)))
            .collect();
        let layout = layout_graph(graph(nodes, edges), LayoutOptions::default());

        assert!(layout.settled, "took all {} passes", layout.iterations);
        assert!(layout.iterations < 300);
    }

    #[test]
    fn the_bounding_box_contains_every_node() {
        let nodes: Vec<GraphNode> = (0..12).map(|i| node(&format!("n{i}"), i)).collect();
        let layout = layout_graph(graph(nodes, Vec::new()), LayoutOptions::default());

        for position in &layout.positions {
            assert!(
                position.x - position.radius >= layout.min_x - 0.01,
                "{position:?}"
            );
            assert!(
                position.x + position.radius <= layout.max_x + 0.01,
                "{position:?}"
            );
            assert!(
                position.y - position.radius >= layout.min_y - 0.01,
                "{position:?}"
            );
            assert!(
                position.y + position.radius <= layout.max_y + 0.01,
                "{position:?}"
            );
        }
    }

    #[test]
    fn a_well_connected_note_is_drawn_larger_but_not_absurdly() {
        let nodes = vec![node("quiet", 0), node("busy", 4), node("hub", 400)];
        let layout = layout_graph(graph(nodes, Vec::new()), LayoutOptions::default());
        let radius = |id: &str| layout.positions.iter().find(|p| p.id == id).unwrap().radius;

        assert!(radius("quiet") < radius("busy"));
        assert!(radius("busy") < radius("hub"));
        // Capped, or one hub swamps everything else on a phone screen.
        assert!(radius("hub") <= 24.0);
    }

    #[test]
    fn an_edge_naming_a_node_that_is_not_there_is_ignored() {
        // A truncated graph keeps edges whose other end was cut. Indexing
        // blindly would panic.
        let layout = layout_graph(
            graph(
                vec![node("a", 1)],
                vec![edge("a", "missing"), edge("gone", "a")],
            ),
            LayoutOptions::default(),
        );
        assert_eq!(layout.positions.len(), 1);
        assert!(layout.positions[0].x.is_finite());
    }

    #[test]
    fn a_self_link_does_not_trap_the_simulation() {
        let layout = layout_graph(
            graph(vec![node("a", 1), node("b", 0)], vec![edge("a", "a")]),
            LayoutOptions::default(),
        );
        for position in &layout.positions {
            assert!(
                position.x.is_finite() && position.y.is_finite(),
                "{position:?}"
            );
        }
    }

    #[test]
    fn a_few_hundred_notes_lay_out_fast_enough_for_a_phone() {
        let nodes: Vec<GraphNode> = (0..300).map(|i| node(&format!("n{i}"), 2)).collect();
        let edges: Vec<GraphEdge> = (0..300)
            .map(|i| edge(&format!("n{i}"), &format!("n{}", (i * 7 + 1) % 300)))
            .collect();

        let started = std::time::Instant::now();
        let layout = layout_graph(graph(nodes, edges), LayoutOptions::default());
        let elapsed = started.elapsed();

        assert_eq!(layout.positions.len(), 300);
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "300 nodes took {elapsed:?}"
        );
    }
}
