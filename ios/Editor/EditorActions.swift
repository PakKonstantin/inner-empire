import Foundation

/// The accessory bar's buttons, and what each one does to the text.
///
/// Every case delegates to the core. That is not indirection for its own sake:
/// the rules — pressing bold twice undoes it, a task walks three states, a
/// heading at the level it already has is removed — are the same rules the
/// desktop follows, and having them in one tested place is what stops the two
/// platforms disagreeing about what a button means.
enum EditorAction: String, CaseIterable, Identifiable {
    case heading
    case bold
    case italic
    case quote
    case bullet
    case task
    case wikilink
    case tag
    case code

    var id: String { rawValue }

    /// The glyph on the button. Every one is paired with an accessibility
    /// label below, because an icon alone is not a name.
    var systemImage: String {
        switch self {
        case .heading: "textformat.size"
        case .bold: "bold"
        case .italic: "italic"
        case .quote: "text.quote"
        case .bullet: "list.bullet"
        case .task: "checklist"
        case .wikilink: "link"
        case .tag: "number"
        case .code: "chevron.left.forwardslash.chevron.right"
        }
    }

    var label: String {
        switch self {
        case .heading: String(localized: "Heading")
        case .bold: String(localized: "Bold")
        case .italic: String(localized: "Italic")
        case .quote: String(localized: "Quote")
        case .bullet: String(localized: "Bulleted list")
        case .task: String(localized: "Checklist")
        case .wikilink: String(localized: "Link to a note")
        case .tag: String(localized: "Tag")
        case .code: String(localized: "Code")
        }
    }

    /// The order the bar shows them in, narrowest first, so the ones that
    /// matter most survive a small screen without scrolling.
    static var barOrder: [EditorAction] {
        [.heading, .bold, .italic, .wikilink, .tag, .bullet, .task, .quote, .code]
    }

    func apply(to text: String, selection: Selection, headingLevel: UInt8 = 2) -> EditResult {
        switch self {
        case .heading: setHeadingLevel(text: text, selection: selection, level: headingLevel)
        case .bold: toggleWrap(text: text, selection: selection, marker: "**")
        case .italic: toggleWrap(text: text, selection: selection, marker: "*")
        case .code: toggleWrap(text: text, selection: selection, marker: "`")
        case .quote: toggleQuote(text: text, selection: selection)
        case .bullet: toggleBullet(text: text, selection: selection)
        case .task: toggleTask(text: text, selection: selection)
        case .wikilink: insertWikilink(text: text, selection: selection)
        case .tag: insertTag(text: text, selection: selection)
        }
    }
}
