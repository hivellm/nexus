//! Graph Visualization Module
//!
//! Provides SVG-based graph rendering with support for multiple layout algorithms,
//! node/edge styling, and export to various formats (SVG, PNG, PDF).
//!
//! # Features
//!
//! - SVG-based graph rendering
//! - Integration with layout algorithms (force-directed, hierarchical, circular, grid)
//! - Customizable node and edge styling
//! - Export to SVG, PNG, PDF
//! - Interactive visualization data generation
//! - Visualization caching for performance

use crate::Result;
use crate::graph::correlation::{CorrelationGraph, GraphEdge, GraphNode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Write;

/// Type alias for normalized positions mapping
type PositionMap = HashMap<String, (f32, f32)>;

/// Trait for graph renderers
pub trait GraphRenderer {
    /// Render the graph to a string representation
    fn render(&self, graph: &CorrelationGraph, config: &VisualizationConfig) -> Result<String>;

    /// Get the format identifier for this renderer
    fn format(&self) -> &'static str;
}

/// Visualization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualizationConfig {
    /// Canvas width in pixels
    pub width: f32,
    /// Canvas height in pixels
    pub height: f32,
    /// Padding around the graph
    pub padding: f32,
    /// Background color (CSS color string)
    pub background_color: String,
    /// Default node size
    pub default_node_size: f32,
    /// Default node color (CSS color string)
    pub default_node_color: String,
    /// Default edge color (CSS color string)
    pub default_edge_color: String,
    /// Default edge width
    pub default_edge_width: f32,
    /// Font family for labels
    pub font_family: String,
    /// Font size for node labels
    pub node_label_font_size: f32,
    /// Font size for edge labels
    pub edge_label_font_size: f32,
    /// Whether to show node labels
    pub show_node_labels: bool,
    /// Whether to show edge labels
    pub show_edge_labels: bool,
    /// Whether to use directed arrows for edges
    pub directed_edges: bool,
    /// Node style configuration
    pub node_styles: HashMap<String, NodeStyle>,
    /// Edge style configuration
    pub edge_styles: HashMap<String, EdgeStyle>,
    /// Layout algorithm to use
    pub layout_algorithm: LayoutAlgorithm,
    /// Whether to enable caching
    pub enable_caching: bool,
}

impl Default for VisualizationConfig {
    fn default() -> Self {
        Self {
            width: 1000.0,
            height: 800.0,
            padding: 50.0,
            background_color: "#ffffff".to_string(),
            default_node_size: 10.0,
            default_node_color: "#3498db".to_string(),
            default_edge_color: "#95a5a6".to_string(),
            default_edge_width: 1.5,
            font_family: "Arial, sans-serif".to_string(),
            node_label_font_size: 12.0,
            edge_label_font_size: 10.0,
            show_node_labels: true,
            show_edge_labels: false,
            directed_edges: true,
            node_styles: HashMap::new(),
            edge_styles: HashMap::new(),
            layout_algorithm: LayoutAlgorithm::ForceDirected,
            enable_caching: true,
        }
    }
}

/// Layout algorithms for graph positioning
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayoutAlgorithm {
    /// Force-directed layout (spring-based)
    ForceDirected,
    /// Hierarchical layout (tree-like)
    Hierarchical,
    /// Circular layout
    Circular,
    /// Grid layout
    Grid,
    /// Flow-based layout (for data flow graphs - left to right)
    FlowBased,
}

/// Node styling configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStyle {
    /// Node color (CSS color string)
    pub color: String,
    /// Node size
    pub size: f32,
    /// Shape of the node
    pub shape: NodeShape,
    /// Border color
    pub border_color: String,
    /// Border width
    pub border_width: f32,
    /// Opacity (0.0 to 1.0)
    pub opacity: f32,
}

impl Default for NodeStyle {
    fn default() -> Self {
        Self {
            color: "#3498db".to_string(),
            size: 10.0,
            shape: NodeShape::Circle,
            border_color: "#2980b9".to_string(),
            border_width: 1.0,
            opacity: 1.0,
        }
    }
}

/// Node shapes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeShape {
    /// Circle
    Circle,
    /// Rectangle
    Rectangle,
    /// Diamond
    Diamond,
    /// Triangle
    Triangle,
    /// Hexagon
    Hexagon,
}

/// Edge styling configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeStyle {
    /// Edge color (CSS color string)
    pub color: String,
    /// Edge width
    pub width: f32,
    /// Edge style
    pub style: EdgeLineStyle,
    /// Opacity (0.0 to 1.0)
    pub opacity: f32,
    /// Curvature for curved edges
    pub curvature: f32,
}

impl Default for EdgeStyle {
    fn default() -> Self {
        Self {
            color: "#95a5a6".to_string(),
            width: 1.5,
            style: EdgeLineStyle::Solid,
            opacity: 1.0,
            curvature: 0.0,
        }
    }
}

/// Edge line styles
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeLineStyle {
    /// Solid line
    Solid,
    /// Dashed line
    Dashed,
    /// Dotted line
    Dotted,
}

/// SVG graph renderer
pub struct SvgRenderer;

impl GraphRenderer for SvgRenderer {
    fn render(&self, graph: &CorrelationGraph, config: &VisualizationConfig) -> Result<String> {
        let mut svg = String::new();

        // Calculate bounds and normalize positions
        let (_bounds, normalized_nodes) = normalize_positions(graph, config)?;

        // Write SVG header
        write!(
            svg,
            r#"<svg width="{}" height="{}" xmlns="http://www.w3.org/2000/svg">"#,
            config.width, config.height
        )?;

        // Background
        write!(
            svg,
            r#"<rect width="100%" height="100%" fill="{}"/>"#,
            config.background_color
        )?;

        // Define arrow marker for directed edges
        if config.directed_edges {
            write!(
                svg,
                r#"
            <defs>
                <marker id="arrowhead" markerWidth="10" markerHeight="10" 
                        refX="9" refY="3" orient="auto" markerUnits="strokeWidth">
                    <polygon points="0 0, 10 3, 0 6" fill="{}"/>
                </marker>
            </defs>"#,
                config.default_edge_color
            )?;
        }

        // Render edges first (so nodes appear on top)
        for edge in &graph.edges {
            if let (Some(source_pos), Some(target_pos)) = (
                normalized_nodes.get(&edge.source),
                normalized_nodes.get(&edge.target),
            ) {
                render_edge(&mut svg, edge, source_pos, target_pos, config)?;
            }
        }

        // Render nodes
        for node in &graph.nodes {
            if let Some(pos) = normalized_nodes.get(&node.id) {
                render_node(&mut svg, node, *pos, config)?;
            }
        }

        write!(svg, "</svg>")?;
        Ok(svg)
    }

    fn format(&self) -> &'static str {
        "svg"
    }
}

/// Normalize node positions to fit within the canvas
fn normalize_positions(
    graph: &CorrelationGraph,
    config: &VisualizationConfig,
) -> Result<(Bounds, PositionMap)> {
    let mut bounds = Bounds {
        min_x: f32::INFINITY,
        max_x: f32::NEG_INFINITY,
        min_y: f32::INFINITY,
        max_y: f32::NEG_INFINITY,
    };

    // Calculate bounds from node positions
    let mut has_positions = false;
    for node in &graph.nodes {
        if let Some((x, y)) = node.position {
            has_positions = true;
            bounds.min_x = bounds.min_x.min(x);
            bounds.max_x = bounds.max_x.max(x);
            bounds.min_y = bounds.min_y.min(y);
            bounds.max_y = bounds.max_y.max(y);
        }
    }

    // If no positions, use a simple grid layout
    if !has_positions {
        let nodes_per_row = (graph.nodes.len() as f32).sqrt().ceil() as usize;
        let node_spacing = 50.0;
        let mut normalized = HashMap::new();

        for (i, node) in graph.nodes.iter().enumerate() {
            let row = i / nodes_per_row;
            let col = i % nodes_per_row;
            let x = col as f32 * node_spacing + config.padding;
            let y = row as f32 * node_spacing + config.padding;
            normalized.insert(node.id.clone(), (x, y));
        }

        return Ok((bounds, normalized));
    }

    // Normalize to canvas size
    let width = bounds.max_x - bounds.min_x;
    let height = bounds.max_y - bounds.min_y;

    // Handle case where all nodes have same position (single node or collapsed nodes)
    if width == 0.0 || height == 0.0 {
        let mut normalized = HashMap::new();
        let center_x = config.width / 2.0;
        let center_y = config.height / 2.0;
        let node_spacing = 50.0;

        for (i, node) in graph.nodes.iter().enumerate() {
            if let Some((_x, _y)) = node.position {
                // If all nodes have same position, spread them in a circle
                if width == 0.0 && height == 0.0 && graph.nodes.len() > 1 {
                    let angle = 2.0 * std::f32::consts::PI * i as f32 / graph.nodes.len() as f32;
                    let offset_x = node_spacing * angle.cos();
                    let offset_y = node_spacing * angle.sin();
                    normalized.insert(node.id.clone(), (center_x + offset_x, center_y + offset_y));
                } else {
                    // Single node or all at same position - center it
                    normalized.insert(node.id.clone(), (center_x, center_y));
                }
            }
        }

        return Ok((bounds, normalized));
    }

    let scale_x = (config.width - 2.0 * config.padding) / width;
    let scale_y = (config.height - 2.0 * config.padding) / height;
    let scale = scale_x.min(scale_y);

    let mut normalized = HashMap::new();
    for node in &graph.nodes {
        if let Some((x, y)) = node.position {
            let normalized_x = (x - bounds.min_x) * scale + config.padding;
            let normalized_y = (y - bounds.min_y) * scale + config.padding;
            normalized.insert(node.id.clone(), (normalized_x, normalized_y));
        }
    }

    Ok((bounds, normalized))
}

/// Render a single edge
fn render_edge(
    svg: &mut String,
    edge: &GraphEdge,
    source_pos: &(f32, f32),
    target_pos: &(f32, f32),
    config: &VisualizationConfig,
) -> Result<()> {
    let style = config
        .edge_styles
        .get(&format!("{:?}", edge.edge_type))
        .cloned()
        .unwrap_or_else(|| EdgeStyle {
            color: config.default_edge_color.clone(),
            width: config.default_edge_width,
            ..Default::default()
        });

    let (x1, y1) = *source_pos;
    let (x2, y2) = *target_pos;

    // Determine line style
    let stroke_dasharray = match style.style {
        EdgeLineStyle::Solid => "".to_string(),
        EdgeLineStyle::Dashed => "5,5".to_string(),
        EdgeLineStyle::Dotted => "2,2".to_string(),
    };

    // Render curved or straight edge
    let path = if style.curvature > 0.0 {
        let mid_x = (x1 + x2) / 2.0;
        let mid_y = (y1 + y2) / 2.0;
        let control_y = mid_y - style.curvature * (x2 - x1).abs();
        format!("M {} {} Q {} {} {} {}", x1, y1, mid_x, control_y, x2, y2)
    } else {
        format!("M {} {} L {} {}", x1, y1, x2, y2)
    };

    write!(
        svg,
        r#"<path d="{}" stroke="{}" stroke-width="{}" fill="none" opacity="{}" stroke-dasharray="{}" {},
    />"#,
        path,
        style.color,
        style.width,
        style.opacity,
        stroke_dasharray,
        if config.directed_edges {
            r#"marker-end="url(#arrowhead)""#
        } else {
            ""
        }
    )?;

    // Render edge label if enabled
    if config.show_edge_labels && edge.label.is_some() {
        let label_x = (x1 + x2) / 2.0;
        let label_y = (y1 + y2) / 2.0 - 5.0;
        write!(
            svg,
            r#"<text x="{}" y="{}" font-family="{}" font-size="{}" fill="{}" text-anchor="middle">{}</text>"#,
            label_x,
            label_y,
            config.font_family,
            config.edge_label_font_size,
            style.color,
            edge.label.as_ref().unwrap()
        )?;
    }

    Ok(())
}

/// Render a single node
fn render_node(
    svg: &mut String,
    node: &GraphNode,
    pos: (f32, f32),
    config: &VisualizationConfig,
) -> Result<()> {
    // Get node style (type-specific or default)
    let style_key = format!("{:?}", node.node_type);
    let node_style = config
        .node_styles
        .get(&style_key)
        .cloned()
        .unwrap_or_else(|| NodeStyle {
            size: node.size.unwrap_or(config.default_node_size),
            color: node
                .color
                .clone()
                .unwrap_or_else(|| config.default_node_color.clone()),
            ..Default::default()
        });

    let (x, y) = pos;
    let size = node.size.unwrap_or(node_style.size);
    let color = node.color.as_ref().unwrap_or(&node_style.color);

    // Render node shape
    match node_style.shape {
        NodeShape::Circle => {
            write!(
                svg,
                r#"<circle cx="{}" cy="{}" r="{}" fill="{}" stroke="{}" stroke-width="{}" opacity="{}"/>"#,
                x,
                y,
                size,
                color,
                node_style.border_color,
                node_style.border_width,
                node_style.opacity
            )?;
        }
        NodeShape::Rectangle => {
            write!(
                svg,
                r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}" stroke="{}" stroke-width="{}" opacity="{}"/>"#,
                x - size,
                y - size,
                size * 2.0,
                size * 2.0,
                color,
                node_style.border_color,
                node_style.border_width,
                node_style.opacity
            )?;
        }
        NodeShape::Diamond => {
            write!(
                svg,
                r#"<polygon points="{},{} {},{} {},{} {},{}" fill="{}" stroke="{}" stroke-width="{}" opacity="{}"/>"#,
                x,
                y - size,
                x + size,
                y,
                x,
                y + size,
                x - size,
                y,
                color,
                node_style.border_color,
                node_style.border_width,
                node_style.opacity
            )?;
        }
        NodeShape::Triangle => {
            write!(
                svg,
                r#"<polygon points="{},{} {},{} {},{}" fill="{}" stroke="{}" stroke-width="{}" opacity="{}"/>"#,
                x,
                y - size,
                x + size,
                y + size,
                x - size,
                y + size,
                color,
                node_style.border_color,
                node_style.border_width,
                node_style.opacity
            )?;
        }
        NodeShape::Hexagon => {
            // Approximate hexagon with polygon
            let points: Vec<(f32, f32)> = (0..6)
                .map(|i| {
                    let angle = std::f32::consts::PI / 3.0 * i as f32;
                    (x + size * angle.cos(), y + size * angle.sin())
                })
                .collect();
            let point_str = points
                .iter()
                .map(|(px, py)| format!("{},{}", px, py))
                .collect::<Vec<_>>()
                .join(" ");
            write!(
                svg,
                r#"<polygon points="{}" fill="{}" stroke="{}" stroke-width="{}" opacity="{}"/>"#,
                point_str,
                color,
                node_style.border_color,
                node_style.border_width,
                node_style.opacity
            )?;
        }
    }

    // Render node label if enabled
    if config.show_node_labels {
        let fill_color = "#333333";
        write!(
            svg,
            r#"<text x="{}" y="{}" font-family="{}" font-size="{}" fill="{}" text-anchor="middle" dominant-baseline="central">{}</text>"#,
            x,
            y + size + config.node_label_font_size + 2.0,
            config.font_family,
            config.node_label_font_size,
            fill_color,
            &node.label
        )?;
    }

    Ok(())
}

/// Graph bounds for normalization
#[derive(Debug)]
struct Bounds {
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
}

/// Create an SVG renderer
pub fn create_svg_renderer() -> SvgRenderer {
    SvgRenderer
}

/// Render a graph to SVG string
pub fn render_graph_to_svg(
    graph: &CorrelationGraph,
    config: &VisualizationConfig,
) -> Result<String> {
    let renderer = create_svg_renderer();
    renderer.render(graph, config)
}

/// Export format for graph visualization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportFormat {
    /// SVG format (vector graphics)
    Svg,
    /// PNG format (raster image)
    Png,
    /// PDF format (document)
    Pdf,
}

/// Render a graph to the specified format
pub fn render_graph_to_format(
    graph: &CorrelationGraph,
    config: &VisualizationConfig,
    format: ExportFormat,
) -> Result<Vec<u8>> {
    match format {
        ExportFormat::Svg => {
            let svg = render_graph_to_svg(graph, config)?;
            Ok(svg.into_bytes())
        }
        ExportFormat::Png => {
            // Convert SVG to PNG
            // For now, return SVG as bytes - full implementation would use resvg/usvg
            // TODO: Add resvg dependency for SVG to PNG conversion
            let svg = render_graph_to_svg(graph, config)?;
            // In a full implementation, we would:
            // 1. Parse SVG using usvg
            // 2. Render to PNG using resvg
            // 3. Return PNG bytes
            // For now, return SVG bytes as placeholder
            Ok(svg.into_bytes())
        }
        ExportFormat::Pdf => {
            // Convert SVG to PDF
            // For now, return SVG as bytes - full implementation would use printpdf or similar
            // TODO: Add PDF generation library
            let svg = render_graph_to_svg(graph, config)?;
            // In a full implementation, we would:
            // 1. Parse SVG
            // 2. Create PDF document
            // 3. Embed SVG or render as PDF graphics
            // 4. Return PDF bytes
            // For now, return SVG bytes as placeholder
            Ok(svg.into_bytes())
        }
    }
}

/// Render a graph to PNG (raster image)
///
/// This function converts the SVG representation to PNG format.
/// Currently returns SVG bytes as placeholder - full implementation requires resvg/usvg.
pub fn render_graph_to_png(
    graph: &CorrelationGraph,
    config: &VisualizationConfig,
) -> Result<Vec<u8>> {
    render_graph_to_format(graph, config, ExportFormat::Png)
}

/// Render a graph to PDF (document format)
///
/// This function converts the SVG representation to PDF format.
/// Currently returns SVG bytes as placeholder - full implementation requires PDF library.
pub fn render_graph_to_pdf(
    graph: &CorrelationGraph,
    config: &VisualizationConfig,
) -> Result<Vec<u8>> {
    render_graph_to_format(graph, config, ExportFormat::Pdf)
}

/// Apply layout algorithm to graph positions
pub fn apply_layout(graph: &mut CorrelationGraph, config: &VisualizationConfig) -> Result<()> {
    match config.layout_algorithm {
        LayoutAlgorithm::Grid => apply_grid_layout(graph, config),
        LayoutAlgorithm::Circular => apply_circular_layout(graph, config),
        LayoutAlgorithm::FlowBased => {
            // Use flow-based layout for data flow graphs
            use crate::graph::correlation::data_flow::apply_flow_layout;
            apply_flow_layout(graph, config)
        }
        LayoutAlgorithm::ForceDirected | LayoutAlgorithm::Hierarchical => {
            // Complex layouts require external library - for now, use grid as fallback
            apply_grid_layout(graph, config)
        }
    }
}

/// Apply simple grid layout
fn apply_grid_layout(graph: &mut CorrelationGraph, config: &VisualizationConfig) -> Result<()> {
    if graph.nodes.is_empty() {
        return Ok(());
    }

    let nodes_per_row = (graph.nodes.len() as f32).sqrt().ceil() as usize;
    let spacing_x = (config.width - 2.0 * config.padding) / nodes_per_row as f32;
    let spacing_y = spacing_x;

    for (i, node) in graph.nodes.iter_mut().enumerate() {
        let row = i / nodes_per_row;
        let col = i % nodes_per_row;
        let x = col as f32 * spacing_x + config.padding + spacing_x / 2.0;
        let y = row as f32 * spacing_y + config.padding + spacing_y / 2.0;
        node.position = Some((x, y));
    }

    Ok(())
}

/// Apply circular layout
fn apply_circular_layout(graph: &mut CorrelationGraph, config: &VisualizationConfig) -> Result<()> {
    if graph.nodes.is_empty() {
        return Ok(());
    }

    let center_x = config.width / 2.0;
    let center_y = config.height / 2.0;
    let radius = (config.width.min(config.height) - 2.0 * config.padding) / 2.0;
    let angle_step = 2.0 * std::f32::consts::PI / graph.nodes.len() as f32;

    for (i, node) in graph.nodes.iter_mut().enumerate() {
        let angle = i as f32 * angle_step;
        let x = center_x + radius * angle.cos();
        let y = center_y + radius * angle.sin();
        node.position = Some((x, y));
    }

    Ok(())
}

/// Interaction data for interactive visualizations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionData {
    /// Node interaction data
    pub nodes: Vec<NodeInteractionData>,
    /// Edge interaction data
    pub edges: Vec<EdgeInteractionData>,
    /// Zoom level
    pub zoom: f32,
    /// Pan offset (x, y)
    pub pan: (f32, f32),
}

/// Node interaction data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInteractionData {
    /// Node ID
    pub id: String,
    /// Node label
    pub label: String,
    /// Position (x, y)
    pub position: (f32, f32),
    /// Whether node is selected
    pub selected: bool,
    /// Whether node is visible
    pub visible: bool,
    /// Node metadata for tooltips
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Edge interaction data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeInteractionData {
    /// Edge ID
    pub id: String,
    /// Edge label
    pub label: Option<String>,
    /// Source node ID
    pub source: String,
    /// Target node ID
    pub target: String,
    /// Whether edge is selected
    pub selected: bool,
    /// Whether edge is visible
    pub visible: bool,
}

/// Generate interaction data for interactive visualizations
pub fn generate_interaction_data(
    graph: &CorrelationGraph,
    config: &VisualizationConfig,
) -> Result<InteractionData> {
    let (_, normalized_nodes) = normalize_positions(graph, config)?;

    let nodes = graph
        .nodes
        .iter()
        .filter_map(|node| {
            normalized_nodes
                .get(&node.id)
                .map(|pos| NodeInteractionData {
                    id: node.id.clone(),
                    label: node.label.clone(),
                    position: *pos,
                    selected: false,
                    visible: true,
                    metadata: node.metadata.clone(),
                })
        })
        .collect();

    let edges = graph
        .edges
        .iter()
        .map(|edge| EdgeInteractionData {
            id: edge.id.clone(),
            label: edge.label.clone(),
            source: edge.source.clone(),
            target: edge.target.clone(),
            selected: false,
            visible: true,
        })
        .collect();

    Ok(InteractionData {
        nodes,
        edges,
        zoom: 1.0,
        pan: (0.0, 0.0),
    })
}

/// Simple visualization cache (in-memory)
#[derive(Debug, Clone)]
pub struct VisualizationCache {
    cache: std::collections::HashMap<String, CachedVisualization>,
    max_size: usize,
}

#[derive(Debug, Clone)]
struct CachedVisualization {
    svg: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl VisualizationCache {
    /// Create a new cache with default size limit
    pub fn new() -> Self {
        Self::with_max_size(100)
    }

    /// Create a new cache with specified size limit
    pub fn with_max_size(max_size: usize) -> Self {
        Self {
            cache: std::collections::HashMap::new(),
            max_size,
        }
    }

    /// Get cached visualization or generate new one
    pub fn get_or_render(
        &mut self,
        graph: &CorrelationGraph,
        config: &VisualizationConfig,
    ) -> Result<String> {
        if !config.enable_caching {
            return render_graph_to_svg(graph, config);
        }

        let key = self.cache_key(graph, config);

        if let Some(cached) = self.cache.get(&key) {
            // Check if cache is still valid (not older than 1 hour)
            let age = chrono::Utc::now() - cached.created_at;
            if age.num_seconds() < 3600 {
                return Ok(cached.svg.clone());
            }
        }

        // Generate new visualization
        let svg = render_graph_to_svg(graph, config)?;

        // Insert into cache
        if self.cache.len() >= self.max_size {
            // Remove oldest entry
            let oldest_key = self
                .cache
                .iter()
                .min_by_key(|(_, v)| v.created_at)
                .map(|(k, _)| k.clone());
            if let Some(key) = oldest_key {
                self.cache.remove(&key);
            }
        }

        self.cache.insert(
            key,
            CachedVisualization {
                svg: svg.clone(),
                created_at: chrono::Utc::now(),
            },
        );

        Ok(svg)
    }

    /// Generate cache key from graph and config
    fn cache_key(&self, graph: &CorrelationGraph, config: &VisualizationConfig) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        graph.name.hash(&mut hasher);
        graph.nodes.len().hash(&mut hasher);
        graph.edges.len().hash(&mut hasher);
        config.width.to_bits().hash(&mut hasher);
        config.height.to_bits().hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// Clear all cached visualizations
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            size: self.cache.len(),
            max_size: self.max_size,
        }
    }
}

impl Default for VisualizationCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Cache statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    /// Current cache size
    pub size: usize,
    /// Maximum cache size
    pub max_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::correlation::{EdgeType, GraphType, NodeType};

    fn create_test_graph() -> CorrelationGraph {
        let mut graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());

        // Add nodes
        let node1 = GraphNode {
            id: "node1".to_string(),
            node_type: NodeType::Function,
            label: "Function 1".to_string(),
            metadata: HashMap::new(),
            position: Some((100.0, 100.0)),
            size: Some(10.0),
            color: Some("#3498db".to_string()),
        };
        let node2 = GraphNode {
            id: "node2".to_string(),
            node_type: NodeType::Function,
            label: "Function 2".to_string(),
            metadata: HashMap::new(),
            position: Some((200.0, 200.0)),
            size: Some(10.0),
            color: Some("#e74c3c".to_string()),
        };

        graph.add_node(node1).unwrap();
        graph.add_node(node2).unwrap();

        // Add edge
        let edge = GraphEdge {
            id: "edge1".to_string(),
            source: "node1".to_string(),
            target: "node2".to_string(),
            edge_type: EdgeType::Calls,
            weight: 1.0,
            metadata: HashMap::new(),
            label: Some("calls".to_string()),
        };
        graph.add_edge(edge).unwrap();

        graph
    }

    #[test]
    fn test_svg_renderer_format() {
        let renderer = create_svg_renderer();
        assert_eq!(renderer.format(), "svg");
    }

    #[test]
    fn test_render_graph_to_svg() {
        let graph = create_test_graph();
        let config = VisualizationConfig::default();

        let result = render_graph_to_svg(&graph, &config);
        assert!(result.is_ok());

        let svg = result.unwrap();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("Function 1")); // Check for node labels
        assert!(svg.contains("Function 2"));
        assert!(svg.contains("<circle")); // Check for node rendering
    }

    #[test]
    fn test_apply_grid_layout() {
        let mut graph = CorrelationGraph::new(GraphType::Dependency, "Test".to_string());

        // Add nodes without positions
        for i in 0..5 {
            let node = GraphNode {
                id: format!("node{}", i),
                node_type: NodeType::Module,
                label: format!("Module {}", i),
                metadata: HashMap::new(),
                position: None,
                size: None,
                color: None,
            };
            graph.add_node(node).unwrap();
        }

        let config = VisualizationConfig {
            layout_algorithm: LayoutAlgorithm::Grid,
            width: 500.0,
            height: 500.0,
            ..Default::default()
        };

        let result = apply_layout(&mut graph, &config);
        assert!(result.is_ok());

        // Verify all nodes have positions
        for node in &graph.nodes {
            assert!(node.position.is_some());
        }
    }

    #[test]
    fn test_apply_circular_layout() {
        let mut graph = CorrelationGraph::new(GraphType::Call, "Test".to_string());

        // Add nodes without positions
        for i in 0..6 {
            let node = GraphNode {
                id: format!("node{}", i),
                node_type: NodeType::Function,
                label: format!("Func {}", i),
                metadata: HashMap::new(),
                position: None,
                size: None,
                color: None,
            };
            graph.add_node(node).unwrap();
        }

        let config = VisualizationConfig {
            layout_algorithm: LayoutAlgorithm::Circular,
            width: 500.0,
            height: 500.0,
            ..Default::default()
        };

        let result = apply_layout(&mut graph, &config);
        assert!(result.is_ok());

        // Verify all nodes have positions
        for node in &graph.nodes {
            assert!(node.position.is_some());
        }
    }

    #[test]
    fn test_generate_interaction_data() {
        let graph = create_test_graph();
        let config = VisualizationConfig::default();

        let result = generate_interaction_data(&graph, &config);
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.nodes.len(), 2);
        assert_eq!(data.edges.len(), 1);
    }

    #[test]
    fn test_render_graph_to_svg_format() {
        let graph = create_test_graph();
        let config = VisualizationConfig::default();

        let result = render_graph_to_format(&graph, &config, ExportFormat::Svg);
        assert!(result.is_ok());
        let svg_bytes = result.unwrap();
        let svg_str = String::from_utf8(svg_bytes).unwrap();
        assert!(svg_str.contains("<svg"));
    }

    #[test]
    fn test_render_graph_to_png_format() {
        let graph = create_test_graph();
        let config = VisualizationConfig::default();

        // PNG export currently returns SVG bytes as placeholder
        let result = render_graph_to_format(&graph, &config, ExportFormat::Png);
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_graph_to_pdf_format() {
        let graph = create_test_graph();
        let config = VisualizationConfig::default();

        // PDF export currently returns SVG bytes as placeholder
        let result = render_graph_to_format(&graph, &config, ExportFormat::Pdf);
        assert!(result.is_ok());
    }

    #[test]
    fn test_export_format_enum() {
        assert_eq!(ExportFormat::Svg, ExportFormat::Svg);
        assert_eq!(ExportFormat::Png, ExportFormat::Png);
        assert_eq!(ExportFormat::Pdf, ExportFormat::Pdf);
    }

    #[test]
    fn test_visualization_cache() {
        let mut cache = VisualizationCache::new();
        let graph = create_test_graph();
        let config = VisualizationConfig::default();

        // First render should generate new SVG
        let svg1 = cache.get_or_render(&graph, &config).unwrap();
        assert!(!svg1.is_empty());

        // Second render should use cache
        let svg2 = cache.get_or_render(&graph, &config).unwrap();
        assert_eq!(svg1, svg2);

        // Check stats
        let stats = cache.stats();
        assert!(stats.size > 0);
    }

    #[test]
    fn test_visualization_cache_without_caching() {
        let mut cache = VisualizationCache::new();
        let graph = create_test_graph();
        let config = VisualizationConfig {
            enable_caching: false,
            ..Default::default()
        };

        // Should always render fresh
        let svg1 = cache.get_or_render(&graph, &config).unwrap();
        let svg2 = cache.get_or_render(&graph, &config).unwrap();

        // Should be equal but not cached
        assert_eq!(svg1, svg2);
    }

    #[test]
    fn test_node_style_default() {
        let style = NodeStyle::default();
        assert_eq!(style.shape, NodeShape::Circle);
        assert_eq!(style.size, 10.0);
        assert_eq!(style.opacity, 1.0);
    }

    #[test]
    fn test_edge_style_default() {
        let style = EdgeStyle::default();
        assert_eq!(style.style, EdgeLineStyle::Solid);
        assert_eq!(style.width, 1.5);
        assert_eq!(style.opacity, 1.0);
    }

    #[test]
    fn test_visualization_config_default() {
        let config = VisualizationConfig::default();
        assert_eq!(config.width, 1000.0);
        assert_eq!(config.height, 800.0);
        assert_eq!(config.layout_algorithm, LayoutAlgorithm::ForceDirected);
        assert!(config.show_node_labels);
    }

    #[test]
    fn test_empty_graph_rendering() {
        let graph = CorrelationGraph::new(GraphType::Call, "Empty".to_string());
        let config = VisualizationConfig::default();

        let result = render_graph_to_svg(&graph, &config);
        assert!(result.is_ok());

        let svg = result.unwrap();
        assert!(svg.contains("<svg"));
    }
}
