import SwiftUI

/// Editing a note's frontmatter.
///
/// The typing rules are the core's: whether `2026-09-17` is a date or a string
/// is decided by `inferPropertyValue`, the same call the desktop's rules come
/// from. Guessing here instead would mean the same note gaining and losing
/// quotes as it moved between platforms, and a `date:` filter matching on one
/// and not the other.
struct PropertiesView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    let path: String

    @State private var properties: [Property] = []
    @State private var loaded = false
    @State private var saving = false
    @State private var failure: String?
    @State private var addingKey = ""
    @State private var knownKeys: [PropertyKeyCount] = []

    var body: some View {
        NavigationStack {
            List {
                if properties.isEmpty && loaded {
                    ContentUnavailableView(
                        "No properties yet",
                        systemImage: "list.bullet.rectangle",
                        description: Text("Properties are the YAML at the top of a note.")
                    )
                }

                ForEach($properties, id: \.key) { $property in
                    PropertyRow(property: $property)
                }
                .onDelete { offsets in
                    properties.remove(atOffsets: offsets)
                }

                Section {
                    HStack {
                        TextField("New property", text: $addingKey)
                            .autocorrectionDisabled()
                            .textInputAutocapitalization(.never)
                        Button("Add") { add() }
                            .disabled(addingKey.trimmingCharacters(in: .whitespaces).isEmpty)
                    }
                    // Names already in the vault, so it does not accumulate
                    // `status`, `Status` and `state` meaning the same thing.
                    ForEach(suggestions, id: \.key) { suggestion in
                        Button {
                            addingKey = suggestion.key
                            add()
                        } label: {
                            HStack {
                                Text(suggestion.key)
                                Spacer()
                                Text("\(Int(suggestion.count))")
                                    .font(.caption.monospacedDigit())
                                    .foregroundStyle(DesignTokens.textMuted.color)
                            }
                        }
                        .accessibilityLabel(Text("Add \(suggestion.key), used in \(Int(suggestion.count)) notes"))
                    }
                } footer: {
                    Text("Names are lowercase by convention. The ones already used in this vault are listed with how many notes carry them.")
                }

                if let failure {
                    Label(failure, systemImage: "exclamationmark.triangle")
                        .foregroundStyle(DesignTokens.textError.color)
                }
            }
            .navigationTitle("Properties")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { Task { await save() } }
                        .disabled(saving || !loaded)
                }
            }
            .task { await load() }
        }
    }

    /// Names the vault already uses that this note does not, narrowed by
    /// whatever has been typed.
    private var suggestions: [PropertyKeyCount] {
        let used = Set(properties.map(\.key))
        let typed = addingKey.trimmingCharacters(in: .whitespaces).lowercased()
        return knownKeys
            .filter { !used.contains($0.key) }
            .filter { typed.isEmpty || $0.key.lowercased().hasPrefix(typed) }
            .prefix(6)
            .map { $0 }
    }

    private func add() {
        let key = addingKey.trimmingCharacters(in: .whitespaces)
        guard !properties.contains(where: { $0.key == key }) else {
            failure = String(localized: "There is already a property called \(key).")
            return
        }
        // A new property starts empty rather than as text: the type follows
        // what gets typed into it, which is how YAML behaves.
        properties.append(Property(key: key, value: .null))
        addingKey = ""
        failure = nil
    }

    private func load() async {
        properties = (try? await model.service.note(at: path).properties) ?? []
        knownKeys = (try? await model.service.propertyKeys()) ?? []
        loaded = true
    }

    private func save() async {
        saving = true
        defer { saving = false }
        do {
            try await model.service.setProperties(properties, on: path)
            dismiss()
        } catch {
            failure = error.localizedDescription
        }
    }
}

/// One property, with the editor its type calls for.
private struct PropertyRow: View {
    @Binding var property: Property
    @State private var text = ""
    @State private var editingText = false

    var body: some View {
        switch propertyValueKind(value: property.value) {
        case .checkbox:
            Toggle(property.key, isOn: Binding(
                get: { if case .checkbox(let on) = property.value { on } else { false } },
                set: { property = Property(key: property.key, value: .checkbox(value: $0)) }
            ))
        case .date, .dateTime:
            DatePicker(
                property.key,
                selection: Binding(
                    get: { parsedDate ?? .now },
                    set: { property = Property(key: property.key, value: formatted($0)) }
                ),
                displayedComponents: propertyValueKind(value: property.value) == .date
                    ? [.date]
                    : [.date, .hourAndMinute]
            )
        case .list:
            // Lists are edited as comma-separated text, which is how they are
            // typed in practice; a row-per-item editor on a phone is more
            // taps than it is worth for a two-item tag list.
            LabeledContent(property.key) {
                TextField("a, b, c", text: $text)
                    .multilineTextAlignment(.trailing)
                    .onAppear { text = listText }
                    .onChange(of: text) { _, value in
                        property = Property(
                            key: property.key,
                            value: .list(values: value
                                .split(separator: ",")
                                .map { inferPropertyValue(raw: $0.trimmingCharacters(in: .whitespaces)) })
                        )
                    }
            }
        case .object:
            // Nested YAML is not editable here, and pretending otherwise would
            // risk flattening it on save. Shown, not touched.
            LabeledContent(property.key) {
                Text("Nested — edit on the desktop")
                    .foregroundStyle(DesignTokens.textMuted.color)
                    .font(.caption)
            }
        case .text, .number, .null:
            LabeledContent(property.key) {
                TextField("Value", text: $text)
                    .multilineTextAlignment(.trailing)
                    .onAppear { text = propertyValueAsText(value: property.value) }
                    .onChange(of: text) { _, value in
                        // The core decides what the typed text means, so iOS
                        // and the desktop cannot disagree about it.
                        property = Property(key: property.key, value: inferPropertyValue(raw: value))
                    }
            }
        }
    }

    private var listText: String {
        guard case .list(let values) = property.value else { return "" }
        return values.map { propertyValueAsText(value: $0) }.joined(separator: ", ")
    }

    private var parsedDate: Date? {
        let raw = propertyValueAsText(value: property.value)
        return ISO8601DateFormatter.vaultDate.date(from: raw)
            ?? ISO8601DateFormatter.vaultDateTime.date(from: raw)
    }

    private func formatted(_ date: Date) -> PropertyValue {
        let isDate = propertyValueKind(value: property.value) == .date
        let text = isDate
            ? ISO8601DateFormatter.vaultDate.string(from: date)
            : ISO8601DateFormatter.vaultDateTime.string(from: date)
        return inferPropertyValue(raw: text)
    }
}

private extension ISO8601DateFormatter {
    /// `2026-09-17`, which is what the core writes and reads.
    static let vaultDate: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withFullDate]
        return formatter
    }()

    static let vaultDateTime: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withFullDate, .withTime, .withColonSeparatorInTime]
        return formatter
    }()
}
