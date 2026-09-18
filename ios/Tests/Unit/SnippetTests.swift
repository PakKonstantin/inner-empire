import Testing
@testable import InnerEmpire

/// The core marks search matches with two control characters rather than
/// `<mark>`, so a native client can build an attributed string and a note
/// containing the literal text `<mark>` is not mistaken for a highlight.
struct SnippetTests {
    /// The parsing, separated from the view so it can be asserted on.
    private func runs(_ snippet: String) -> [(String, Bool)] {
        var out: [(String, Bool)] = []
        var highlighted = false
        for run in snippet.split(separator: "\u{2}", omittingEmptySubsequences: false) {
            for part in run.split(separator: "\u{3}", omittingEmptySubsequences: false) {
                if !part.isEmpty { out.append((String(part), highlighted)) }
                highlighted = false
            }
            highlighted = true
        }
        return out
    }

    @Test func plainTextHasNoHighlights() {
        let parsed = runs("the quick brown fox")
        #expect(parsed.count == 1)
        #expect(parsed[0].0 == "the quick brown fox")
        #expect(parsed[0].1 == false)
    }

    @Test func aMatchIsMarked() {
        let parsed = runs("the quick \u{2}brown\u{3} fox")
        #expect(parsed.map(\.0) == ["the quick ", "brown", " fox"])
        #expect(parsed.map(\.1) == [false, true, false])
    }

    @Test func severalMatchesAreEachMarked() {
        let parsed = runs("\u{2}the\u{3} quick \u{2}brown\u{3} fox")
        #expect(parsed.map(\.0) == ["the", " quick ", "brown", " fox"])
        #expect(parsed.map(\.1) == [true, false, true, false])
    }

    @Test func markupInTheNoteItselfIsNotAHighlight() {
        // The reason for control characters rather than HTML.
        let parsed = runs("a note about <mark> tags")
        #expect(parsed.count == 1)
        #expect(parsed[0].1 == false)
    }
}
