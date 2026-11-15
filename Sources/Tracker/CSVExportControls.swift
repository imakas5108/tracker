import SwiftUI

@MainActor
final class CSVExportPreferences: ObservableObject {
    @Published var profile: CSVProfile = .excel {
        didSet {
            guard profile != oldValue else { return }
            decimalSeparator = profile == .excel ? .recommended(for:.current) : .dot
            includeProvenance = profile == .standard
        }
    }
    @Published var decimalSeparator = CSVDecimalSeparator.recommended(for:.current)
    @Published var includeProvenance = false
    @Published var decimalPlaces = -1
    var options: CSVExportOptions {
        CSVExportOptions(profile:profile,decimalSeparator:profile == .standard ? .dot : decimalSeparator,
            includeProvenance:includeProvenance,decimalPlaces:decimalPlaces < 0 ? nil : decimalPlaces)
    }
}

struct CSVExportControls: View {
    @ObservedObject var preferences: CSVExportPreferences
    var body: some View {
        Form {
            Picker("Format",selection:$preferences.profile) {
                ForEach(CSVProfile.allCases) { Text($0.title).tag($0) }
            }
            Picker("Decimal separator",selection:$preferences.decimalSeparator) {
                ForEach(CSVDecimalSeparator.allCases) { Text($0.title).tag($0) }
            }.disabled(preferences.profile == .standard)
            Picker("Precision",selection:$preferences.decimalPlaces) {
                Text("Full precision").tag(-1)
                Text("6 decimal places (rounded)").tag(6)
                Text("9 decimal places (rounded)").tag(9)
                Text("12 decimal places (rounded)").tag(12)
            }
            Toggle("Include provenance and processing details",isOn:$preferences.includeProvenance)
            Text(preferences.profile == .excel ? "Excel CSV uses short headers and your chosen regional separators." : "Standard CSV uses commas, decimal points and machine-readable headers.")
                .font(.caption).foregroundStyle(.secondary)
            if preferences.decimalPlaces >= 0 {
                Text("Rounding affects this export only; project measurements keep their full precision.")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }.formStyle(.grouped).frame(width:430).padding(8)
    }
}
