import SwiftUI
import UIKit

/// A `UITextView`, because SwiftUI's `TextEditor` cannot do what a Markdown
/// editor needs: a keyboard accessory, selection the app can read and set, and
/// syntax attributes applied as you type.
///
/// Offsets cross to the core as UTF-16, which is what `NSRange` speaks. Getting
/// that wrong is not a cosmetic bug — an edit applied at a byte offset in a
/// note containing an emoji corrupts it — so the conversion happens in one
/// place in Rust and this type never does arithmetic on offsets itself.
struct MarkdownTextView: UIViewRepresentable {
    @Binding var text: String
    @Binding var selection: Selection
    let isEditable: Bool
    /// Applied to the text view so the accessory bar can drive it.
    let accessory: UIView?

    func makeUIView(context: Context) -> UITextView {
        let view = UITextView()
        view.delegate = context.coordinator
        view.isEditable = isEditable
        view.alwaysBounceVertical = true
        view.keyboardDismissMode = .interactive
        view.textContainerInset = UIEdgeInsets(
            top: DesignTokens.spacingMd,
            left: DesignTokens.spacingMd,
            bottom: DesignTokens.spacingLg,
            right: DesignTokens.spacingMd
        )
        view.backgroundColor = .clear
        // Markdown is plain text; the system's substitutions turn "--" into an
        // em dash and quotes into curly ones, which changes what is written to
        // the file without the user asking.
        view.smartDashesType = .no
        view.smartQuotesType = .no
        view.smartInsertDeleteType = .no
        view.autocorrectionType = .default
        view.inputAccessoryView = accessory

        // Dynamic Type: the editor follows the user's size, and the monospaced
        // face keeps Markdown's alignment legible while it does.
        view.font = UIFontMetrics(forTextStyle: .body)
            .scaledFont(for: .monospacedSystemFont(ofSize: 16, weight: .regular))
        view.adjustsFontForContentSizeCategory = true

        view.accessibilityLabel = String(localized: "Note text")
        view.text = text
        return view
    }

    func updateUIView(_ view: UITextView, context: Context) {
        context.coordinator.parent = self

        // Only when it actually differs: assigning `text` resets the selection
        // and dismisses the autocorrect bar, which mid-typing is maddening.
        if view.text != text {
            let previous = view.selectedRange
            view.text = text
            view.selectedRange = NSRange(
                location: min(previous.location, (view.text as NSString).length),
                length: 0
            )
        }

        let wanted = NSRange(
            location: Int(selection.start),
            length: Int(selection.end) - Int(selection.start)
        )
        if view.selectedRange != wanted,
           NSMaxRange(wanted) <= (view.text as NSString).length {
            view.selectedRange = wanted
        }

        view.isEditable = isEditable
        if view.inputAccessoryView !== accessory {
            view.inputAccessoryView = accessory
            view.reloadInputViews()
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    final class Coordinator: NSObject, UITextViewDelegate {
        var parent: MarkdownTextView

        init(parent: MarkdownTextView) {
            self.parent = parent
        }

        func textViewDidChange(_ textView: UITextView) {
            parent.text = textView.text
            report(textView)
        }

        func textViewDidChangeSelection(_ textView: UITextView) {
            report(textView)
        }

        private func report(_ textView: UITextView) {
            let range = textView.selectedRange
            let updated = Selection(
                start: UInt32(range.location),
                end: UInt32(NSMaxRange(range))
            )
            if parent.selection != updated {
                parent.selection = updated
            }
        }
    }
}
