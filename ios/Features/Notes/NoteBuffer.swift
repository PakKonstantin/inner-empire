import Foundation
import OSLog
import SwiftUI

/// One note being edited.
///
/// The important state is `baseModifiedMs`: what the file said when this
/// buffer was loaded. Every save carries it, and the core refuses the write if
/// the file has moved on. That is the whole of §61 — an edit made in the Files
/// app, by iCloud, or on a Mac sharing the folder cannot be overwritten
/// without the user seeing it first.
@MainActor
@Observable
final class NoteBuffer {
    enum SaveState: Equatable {
        case saved
        case dirty
        case saving
        /// The file changed underneath. Nothing is written until this is
        /// resolved; the text the user typed is still here.
        case conflicted(remote: String)
        case failed(String)
    }

    let path: String
    private(set) var title: String
    private(set) var state: SaveState = .saved

    /// The editor's text. Assigning marks the buffer dirty and restarts the
    /// autosave timer.
    var text: String {
        didSet {
            guard text != oldValue else { return }
            if case .conflicted = state {
                // Still conflicted: typing more does not resolve anything.
                scheduleJournal()
                return
            }
            state = .dirty
            scheduleAutosave()
            scheduleJournal()
        }
    }

    var selection = Selection(start: 0, end: 0)

    private var baseModifiedMs: Int64
    private let service: VaultService
    private var autosave: Task<Void, Never>?
    private var journalling: Task<Void, Never>?
    private let log = Logger(subsystem: Logging.subsystem, category: "buffer")

    /// Long enough that a pause between words does not write, short enough
    /// that work is never far from durable.
    private let autosaveDelay = Duration.seconds(2)
    /// Shorter: the journal is the crash net, and it is cheap.
    private let journalDelay = Duration.milliseconds(600)

    init(note: Note, service: VaultService) {
        self.path = note.path
        self.title = note.title
        self.text = note.content
        self.baseModifiedMs = note.modifiedMs
        self.service = service
    }

    // MARK: - Saving

    /// Write now, if there is anything to write.
    func saveIfNeeded() async {
        guard state == .dirty || state == .saving else { return }
        await save()
    }

    private func save() async {
        autosave?.cancel()
        let pending = text
        state = .saving
        do {
            baseModifiedMs = try await service.save(
                pending, to: path, baseModifiedMs: baseModifiedMs
            )
            // Only if nothing was typed while the write was in flight.
            state = (pending == text) ? .saved : .dirty
            if state == .dirty { scheduleAutosave() }
        } catch let error as FfiError {
            await handle(error, attempted: pending)
        } catch {
            state = .failed(error.localizedDescription)
        }
    }

    private func handle(_ error: FfiError, attempted: String) async {
        guard case .ExternalModification = error else {
            log.error("saving failed: \(error.localizedDescription, privacy: .public)")
            state = .failed(error.localizedDescription)
            return
        }

        // Fetch what is actually on disk so the user can compare rather than
        // choose blind. The typed text stays in `text` throughout.
        let remote = (try? await service.note(at: path).content) ?? ""
        log.notice("the note changed on disk while it was open")
        state = .conflicted(remote: remote)
    }

    private func scheduleAutosave() {
        autosave?.cancel()
        autosave = Task { [autosaveDelay] in
            try? await Task.sleep(for: autosaveDelay)
            guard !Task.isCancelled else { return }
            await save()
        }
    }

    // MARK: - Crash safety

    /// Record the unsaved text where a crash cannot take it.
    ///
    /// The journal lives in the app container, not the vault: a half-typed
    /// paragraph is machine-local and has no business syncing to every other
    /// device the moment it is typed.
    private func scheduleJournal() {
        journalling?.cancel()
        journalling = Task { [journalDelay, text, path] in
            try? await Task.sleep(for: journalDelay)
            guard !Task.isCancelled else { return }
            try? await service.journal(text, for: path)
        }
    }

    /// Called as the app loses the foreground. Not debounced: this is the last
    /// moment guaranteed before the system can suspend the process.
    func flush() async {
        journalling?.cancel()
        try? await service.journal(text, for: path)
        await saveIfNeeded()
    }

    // MARK: - Resolving a conflict

    /// Discard what was typed and take the file.
    func reloadFromDisk() async {
        do {
            let note = try await service.note(at: path)
            text = note.content
            title = note.title
            baseModifiedMs = note.modifiedMs
            state = .saved
            try? await service.clearJournal(for: path)
        } catch {
            state = .failed(error.localizedDescription)
        }
    }

    /// Keep what was typed, replacing the file.
    ///
    /// Reachable only from the conflict sheet, where the other version has
    /// already been shown. There is no path from typing to this.
    func overwriteWithLocal() async {
        state = .saving
        do {
            baseModifiedMs = try await service.saveOverwriting(text, to: path)
            state = .saved
        } catch {
            state = .failed(error.localizedDescription)
        }
    }

    /// Write a merged version the user assembled hunk by hunk.
    func resolve(withMerged merged: String) async {
        text = merged
        await overwriteWithLocal()
    }

    // MARK: - Editing

    /// Put text in at the cursor, replacing the selection.
    ///
    /// Offsets are UTF-16 because that is what the text view reports, and the
    /// arithmetic is done on `unicodeScalars`-aware indices rather than by
    /// slicing a `String` by integer — a note with an emoji before the cursor
    /// would otherwise insert in the wrong place.
    func insert(_ markdown: String) {
        let text = self.text as NSString
        let start = Int(min(selection.start, selection.end))
        let end = Int(max(selection.start, selection.end))
        guard start <= text.length, end <= text.length else { return }

        let updated = text.replacingCharacters(
            in: NSRange(location: start, length: end - start),
            with: markdown
        )
        self.text = updated
        let cursor = UInt32(start + (markdown as NSString).length)
        selection = Selection(start: cursor, end: cursor)
    }

    func apply(_ action: EditorAction, headingLevel: UInt8 = 2) {
        let result = action.apply(to: text, selection: selection, headingLevel: headingLevel)
        text = result.text
        selection = Selection(start: result.selectionStart, end: result.selectionEnd)
    }
}
