import SwiftUI

/// Settings, and the place where the vault reports on itself.
struct SettingsView: View {
    @Environment(AppModel.self) private var model
    @AppStorage("appearance") private var appearance = Appearance.system

    enum Appearance: String, CaseIterable, Identifiable {
        case system, light, dark
        var id: String { rawValue }
        var label: LocalizedStringKey {
            switch self {
            case .system: "System"
            case .light: "Light"
            case .dark: "Dark"
            }
        }
    }

    /// A switch expression is only allowed where a value is being bound, not
    /// as a call argument, so it is hoisted rather than inlined.
    private var colorScheme: ColorScheme? {
        switch appearance {
        case .system: nil
        case .light: .light
        case .dark: .dark
        }
    }

    var body: some View {
        Form {
            if let vault = model.vault {
                Section("Vault") {
                    LabeledContent("Name", value: vault.name)
                    LabeledContent("Filenames") {
                        Text(vault.caseSensitive
                             ? String(localized: "Case-sensitive")
                             : String(localized: "Case-insensitive"))
                    }
                }
            }

            Section("Appearance") {
                Picker("Theme", selection: $appearance) {
                    ForEach(Appearance.allCases) { option in
                        Text(option.label).tag(option)
                    }
                }
            }

            if !model.diagnostics.isEmpty {
                Section {
                    ForEach(Array(model.diagnostics.enumerated()), id: \.offset) { _, diagnostic in
                        DiagnosticRow(diagnostic: diagnostic)
                    }
                } header: {
                    Text("Things worth knowing")
                } footer: {
                    Text("None of these stop the vault working. They are names or files that could behave differently on another device.")
                }
            }

            Section {
                LabeledContent("Notes stay on this device") {
                    Image(systemName: "checkmark")
                        .accessibilityLabel(Text("Yes"))
                }
            } header: {
                Text("Privacy")
            } footer: {
                Text("Inner Empire makes no network requests and collects nothing. The search index is stored on this device, outside your vault, so your notes are never uploaded as a side effect of being indexed.")
            }
        }
        .navigationTitle("Settings")
        .preferredColorScheme(colorScheme)
    }
}

/// One diagnostic, said in words rather than a code.
///
/// Every row carries an icon *and* a label: §53 forbids colour as the only
/// carrier of meaning, and an icon alone is only marginally better.
private struct DiagnosticRow: View {
    let diagnostic: Diagnostic

    var body: some View {
        Label {
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.callout)
                Text(detail)
                    .font(.caption)
                    .foregroundStyle(DesignTokens.textMuted.color)
            }
        } icon: {
            Image(systemName: icon)
                .foregroundStyle(DesignTokens.textWarning.color)
        }
        .accessibilityElement(children: .combine)
    }

    private var title: String {
        switch diagnostic {
        case .caseConflict: String(localized: "Two names differ only by capitalisation")
        case .unportableName: String(localized: "A name that Windows would reject")
        case .interruptedWrite: String(localized: "A write was interrupted")
        case .unreadableFile: String(localized: "A file could not be read")
        case .malformedFrontmatter: String(localized: "Properties could not be read")
        case .escapingSymlink: String(localized: "A link points outside the vault")
        }
    }

    private var detail: String {
        switch diagnostic {
        case .caseConflict(let paths): paths.joined(separator: ", ")
        case .unportableName(let path, let reason): "\(path) — \(reason)"
        case .interruptedWrite(let path): path
        case .unreadableFile(let path, let message): "\(path) — \(message)"
        case .malformedFrontmatter(let path, let message): "\(path) — \(message)"
        case .escapingSymlink(let path): path
        }
    }

    private var icon: String {
        switch diagnostic {
        case .caseConflict: "textformat.abc"
        case .unportableName: "character.cursor.ibeam"
        case .interruptedWrite: "exclamationmark.triangle"
        case .unreadableFile: "eye.slash"
        case .malformedFrontmatter: "list.bullet.rectangle"
        case .escapingSymlink: "arrow.up.forward.square"
        }
    }
}
