import Testing
@testable import InnerEmpire

/// Paths, which are the one place a bug becomes a file written outside the
/// vault. The core re-validates everything it is given, so this is not the
/// security boundary — but a path that reaches the core malformed produces a
/// confusing error, and one that never should have been built produces none.
struct VaultPathTests {
    @Test func normalisingRemovesRedundancy() throws {
        #expect(try VaultPathUtil.normalize("Notes/Hub.md") == "Notes/Hub.md")
        #expect(try VaultPathUtil.normalize("/Notes/Hub.md") == "Notes/Hub.md")
        #expect(try VaultPathUtil.normalize("Notes//Hub.md") == "Notes/Hub.md")
        #expect(try VaultPathUtil.normalize("./Notes/./Hub.md") == "Notes/Hub.md")
        #expect(try VaultPathUtil.normalize("") == "")
    }

    @Test func backslashesBecomeSeparators() throws {
        // A path that arrived from a vault authored on Windows, or from a
        // share extension handing over a Windows-style path.
        #expect(try VaultPathUtil.normalize("Notes\\Hub.md") == "Notes/Hub.md")
    }

    @Test func dotDotIsRefusedRatherThanResolved() {
        for escape in ["../outside.md", "Notes/../../outside.md", "..", "a/../../b.md"] {
            #expect(throws: VaultStorageError.escapesVault(escape)) {
                try VaultPathUtil.normalize(escape)
            }
        }
    }

    @Test func namesAreComposedTheSameWayTheCoreComposesThem() throws {
        // "é" typed on iOS is often U+0065 U+0301; the core stores NFC. Left
        // alone, the same filename would compare unequal on the two platforms
        // and the note would look missing.
        let decomposed = "Notes/Cafe\u{0301}.md"
        let normalized = try VaultPathUtil.normalize(decomposed)
        #expect(normalized == "Notes/Café.md".precomposedStringWithCanonicalMapping)
        #expect(normalized.unicodeScalars.count < decomposed.unicodeScalars.count)
    }

    @Test func resolvingStaysUnderTheRoot() throws {
        let root = URL(fileURLWithPath: "/vaults/mine")
        let url = try VaultPathUtil.resolve("Notes/Hub.md", under: root)
        #expect(url.path(percentEncoded: false) == "/vaults/mine/Notes/Hub.md")

        #expect(throws: (any Error).self) {
            try VaultPathUtil.resolve("../elsewhere/Hub.md", under: root)
        }
    }

    @Test func resolvingAnEmptyPathIsTheRoot() throws {
        let root = URL(fileURLWithPath: "/vaults/mine")
        #expect(try VaultPathUtil.resolve("", under: root) == root)
    }

    @Test func relativeIsTheInverseOfResolve() throws {
        let root = URL(fileURLWithPath: "/vaults/mine")
        let url = try VaultPathUtil.resolve("Notes/Deep/Hub.md", under: root)
        #expect(VaultPathUtil.relative(url, under: root) == "Notes/Deep/Hub.md")
    }

    @Test func somethingOutsideTheVaultHasNoRelativeForm() {
        let root = URL(fileURLWithPath: "/vaults/mine")
        let outside = URL(fileURLWithPath: "/etc/passwd")
        #expect(VaultPathUtil.relative(outside, under: root) == nil)
    }

    @Test func aSiblingDirectoryIsNotInsideTheVault() {
        // "/vaults/mine-backup" starts with "/vaults/mine" as a string but is
        // a different folder. A prefix check without the separator would let
        // it through.
        let root = URL(fileURLWithPath: "/vaults/mine")
        let sibling = URL(fileURLWithPath: "/vaults/mine-backup/Hub.md")
        #expect(VaultPathUtil.relative(sibling, under: root) == nil)
    }
}
