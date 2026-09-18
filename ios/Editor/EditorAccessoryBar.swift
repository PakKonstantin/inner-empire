import SwiftUI

/// The bar above the keyboard.
///
/// §16 asks for `[H] [B] [I] ["] [-] [☑] [[ ]] [#]` adapting to the width with
/// horizontal scrolling. It scrolls rather than wrapping or shrinking, because
/// a button that has shrunk below the minimum touch target is worse than one
/// you have to scroll to.
struct EditorAccessoryBar: View {
    let onAction: (EditorAction) -> Void
    let onDismissKeyboard: () -> Void

    var body: some View {
        HStack(spacing: 0) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: DesignTokens.spacingXs) {
                    ForEach(EditorAction.barOrder) { action in
                        Button {
                            onAction(action)
                        } label: {
                            Image(systemName: action.systemImage)
                                .frame(minWidth: 44, minHeight: 44)
                        }
                        // The glyph is decoration; this is the name VoiceOver
                        // reads and the only thing distinguishing the buttons
                        // for anyone not looking at them.
                        .accessibilityLabel(Text(action.label))
                    }
                }
                .padding(.horizontal, DesignTokens.spacingSm)
            }

            Divider()

            Button {
                onDismissKeyboard()
            } label: {
                Image(systemName: "keyboard.chevron.compact.down")
                    .frame(minWidth: 44, minHeight: 44)
            }
            .accessibilityLabel(Text("Hide keyboard"))
            .padding(.horizontal, DesignTokens.spacingSm)
        }
        .background(.bar)
        .frame(height: 48)
    }
}
