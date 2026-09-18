import SwiftUI

/// Placeholder until the graph phase.
///
/// Present so the tab bar is the shape §27 asks for from the start, and so the
/// navigation can be walked end to end. It says what it is rather than showing
/// an empty canvas that looks broken.
struct GraphView: View {
    var body: some View {
        ContentUnavailableView(
            "Graph",
            systemImage: "point.3.connected.trianglepath.dotted",
            description: Text("The graph arrives in a later phase.")
        )
        .navigationTitle("Graph")
    }
}
