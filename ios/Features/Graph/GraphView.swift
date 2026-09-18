import SwiftUI

/// The vault as a picture.
///
/// The layout comes from the core, which means the same vault arranges itself
/// the same way every time it is opened — you can build a mental map of it —
/// and that the arrangement is tested rather than eyeballed.
///
/// Drawn with `Canvas` rather than a view per node: a few hundred SwiftUI
/// views with gestures attached is a few hundred things for the system to lay
/// out on every frame of a pinch.
struct GraphView: View {
    @Environment(AppModel.self) private var model

    /// When set, the graph shows the neighbourhood of this note rather than
    /// the whole vault.
    var centre: String?

    @State private var graph: GraphData?
    @State private var layout: GraphLayout?
    @State private var loading = true
    @State private var depth: UInt32 = 2

    // Viewport. Committed values plus the in-flight gesture, so a pinch and a
    // pan compose without fighting each other.
    @State private var zoom: CGFloat = 1
    @State private var pinch: CGFloat = 1
    @State private var pan: CGSize = .zero
    @State private var drag: CGSize = .zero

    @State private var selected: NodePosition?

    var body: some View {
        GeometryReader { geometry in
            ZStack {
                if let layout, let graph, !layout.positions.isEmpty {
                    canvas(layout: layout, graph: graph, size: geometry.size)
                } else if loading {
                    ProgressView()
                } else {
                    ContentUnavailableView(
                        "Nothing to draw yet",
                        systemImage: "point.3.connected.trianglepath.dotted",
                        description: Text("Link some notes together and they appear here.")
                    )
                }

                if graph?.truncated == true {
                    // Said out loud rather than quietly showing part of the
                    // vault, which would look like notes had gone missing.
                    VStack {
                        Text("Showing part of a large vault")
                            .font(.caption)
                            .padding(.horizontal, DesignTokens.spacingSm)
                            .padding(.vertical, DesignTokens.spacingXs)
                            .background(.thinMaterial, in: Capsule())
                            .padding(.top, DesignTokens.spacingSm)
                        Spacer()
                    }
                }
            }
        }
        .navigationTitle(centre == nil ? "Graph" : "Connections")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if centre != nil {
                ToolbarItem(placement: .topBarTrailing) {
                    Picker("Depth", selection: $depth) {
                        Text("1").tag(UInt32(1))
                        Text("2").tag(UInt32(2))
                        Text("3").tag(UInt32(3))
                    }
                    .pickerStyle(.segmented)
                    .frame(width: 130)
                    .accessibilityLabel(Text("How many links out to show"))
                }
            }
        }
        .sheet(item: $selected) { node in
            GraphNodeSheet(node: node, graph: graph)
        }
        .task(id: depth) { await load() }
    }

    private func canvas(layout: GraphLayout, graph: GraphData, size: CGSize) -> some View {
        let scale = fitScale(layout: layout, into: size) * zoom * pinch
        let offset = CGSize(width: pan.width + drag.width, height: pan.height + drag.height)
        let centreOfBox = CGPoint(
            x: CGFloat(layout.minX + layout.maxX) / 2,
            y: CGFloat(layout.minY + layout.maxY) / 2
        )

        func place(_ position: NodePosition) -> CGPoint {
            CGPoint(
                x: (CGFloat(position.x) - centreOfBox.x) * scale + size.width / 2 + offset.width,
                y: (CGFloat(position.y) - centreOfBox.y) * scale + size.height / 2 + offset.height
            )
        }

        let byID = Dictionary(uniqueKeysWithValues: layout.positions.map { ($0.id, $0) })

        return Canvas { context, _ in
            // Edges first, so nodes sit on top of them.
            for edge in graph.edges {
                guard let from = byID[edge.source], let to = byID[edge.target] else { continue }
                var line = Path()
                line.move(to: place(from))
                line.addLine(to: place(to))
                context.stroke(
                    line,
                    with: .color(DesignTokens.graphEdge.color.opacity(0.5)),
                    lineWidth: 1
                )
            }

            for position in layout.positions {
                let point = place(position)
                let radius = CGFloat(position.radius) * min(scale, 2)
                let rect = CGRect(
                    x: point.x - radius, y: point.y - radius,
                    width: radius * 2, height: radius * 2
                )
                context.fill(Path(ellipseIn: rect), with: .color(colour(for: position, in: graph)))

                // Labels only when they would be readable. Drawing four
                // hundred of them at arm's length is illegible and slow.
                if scale > 0.8, let node = graph.nodes.first(where: { $0.id == position.id }) {
                    context.draw(
                        Text(node.label).font(.caption2).foregroundStyle(DesignTokens.textMuted.color),
                        at: CGPoint(x: point.x, y: point.y + radius + 8)
                    )
                }
            }
        }
        .contentShape(Rectangle())
        .gesture(
            SimultaneousGesture(
                MagnifyGesture()
                    .onChanged { pinch = $0.magnification }
                    .onEnded { _ in
                        // Clamped: zooming to nothing or to a single pixel
                        // leaves no way back.
                        zoom = (zoom * pinch).clamped(to: 0.2...6)
                        pinch = 1
                    },
                DragGesture()
                    .onChanged { drag = $0.translation }
                    .onEnded { _ in
                        pan = CGSize(width: pan.width + drag.width, height: pan.height + drag.height)
                        drag = .zero
                    }
            )
        )
        .onTapGesture { location in
            selected = layout.positions.min {
                place($0).distance(to: location) < place($1).distance(to: location)
            }.flatMap { nearest in
                // Only if the tap actually landed near it: a tap on empty
                // space should do nothing, not open the least-distant note.
                place(nearest).distance(to: location) <= max(CGFloat(nearest.radius) * scale, 22)
                    ? nearest
                    : nil
            }
        }
        .accessibilityLabel(Text("Graph of \(graph.nodes.count) notes and \(graph.edges.count) links"))
        // The picture is the point, and it cannot be read aloud. A list of the
        // busiest notes is the honest alternative to a silent canvas.
        .accessibilityHint(Text("Use the Notes tab to browse these as a list."))
    }

    private func colour(for position: NodePosition, in graph: GraphData) -> Color {
        guard let node = graph.nodes.first(where: { $0.id == position.id }) else {
            return DesignTokens.graphNode.color
        }
        return switch node.kind {
        case .note: DesignTokens.graphNode.color
        case .attachment: DesignTokens.graphNodeAttachment.color
        case .unresolved: DesignTokens.graphNodeUnresolved.color
        case .tag: DesignTokens.graphNodeTag.color
        }
    }

    private func fitScale(layout: GraphLayout, into size: CGSize) -> CGFloat {
        let width = CGFloat(layout.maxX - layout.minX)
        let height = CGFloat(layout.maxY - layout.minY)
        guard width > 0, height > 0 else { return 1 }
        // A margin, so the outermost nodes are not flush against the edge.
        return min(size.width / width, size.height / height) * 0.85
    }

    private func load() async {
        loading = true
        defer { loading = false }

        let options = GraphOptions()
        let data: GraphData?
        if let centre {
            data = try? await model.service.localGraph(around: centre, depth: depth, options: options)
        } else {
            data = try? await model.service.graph(options: options)
        }
        guard let data else { return }
        graph = data
        // Off the main actor: a few hundred nodes is real arithmetic, and the
        // screen should stay responsive while it happens.
        layout = await Task.detached(priority: .userInitiated) {
            layoutGraph(graph: data, options: LayoutOptions())
        }.value
    }
}

/// What a tapped node offers.
private struct GraphNodeSheet: View {
    @Environment(\.dismiss) private var dismiss
    let node: NodePosition
    let graph: GraphData?

    private var graphNode: GraphNode? {
        graph?.nodes.first { $0.id == node.id }
    }

    var body: some View {
        NavigationStack {
            List {
                if let graphNode {
                    Section {
                        LabeledContent("Name", value: graphNode.label)
                        LabeledContent("Links", value: "\(Int(graphNode.degree))")
                        if !graphNode.folder.isEmpty {
                            LabeledContent("Folder", value: graphNode.folder)
                        }
                    }
                    if let path = graphNode.path {
                        NavigationLink(value: path) {
                            Label("Open this note", systemImage: "doc.text")
                        }
                    } else {
                        Label("No note with this name yet", systemImage: "questionmark.circle")
                            .foregroundStyle(DesignTokens.textMuted.color)
                    }
                }
            }
            .navigationTitle(graphNode?.label ?? "Note")
            .navigationBarTitleDisplayMode(.inline)
            .navigationDestination(for: String.self) { NoteDetailPlaceholder(path: $0) }
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .presentationDetents([.medium])
    }
}

// `NodePosition` already carries an `id`, so the conformance needs no body.
// Writing `var id: String { self.id }` here would recurse into itself and
// hang the first time a node was tapped.
extension NodePosition: @retroactive Identifiable {}

private extension CGPoint {
    func distance(to other: CGPoint) -> CGFloat {
        ((x - other.x) * (x - other.x) + (y - other.y) * (y - other.y)).squareRoot()
    }
}

private extension CGFloat {
    func clamped(to range: ClosedRange<CGFloat>) -> CGFloat {
        Swift.min(Swift.max(self, range.lowerBound), range.upperBound)
    }
}
