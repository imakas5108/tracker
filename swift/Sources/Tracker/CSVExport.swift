import Foundation

enum CSVProfile: String, CaseIterable, Identifiable, Sendable {
    case excel, standard
    var id: String { rawValue }
    var title: String { self == .excel ? "Excel CSV" : "Standard CSV" }
}

enum CSVDecimalSeparator: String, CaseIterable, Identifiable, Sendable {
    case dot, comma
    var id: String { rawValue }
    var symbol: String { self == .comma ? "," : "." }
    var title: String { self == .comma ? "Comma (1,25)" : "Point (1.25)" }
    static func recommended(for locale: Locale) -> Self { locale.decimalSeparator == "," ? .comma : .dot }
}

struct CSVExportOptions: Equatable, Sendable {
    var profile: CSVProfile = .standard
    var decimalSeparator: CSVDecimalSeparator = .dot
    var includeProvenance = true
    var decimalPlaces: Int? = nil
    var delimiter: String { profile == .excel && decimalSeparator == .comma ? ";" : "," }
    static func excel(locale: Locale = .current) -> Self {
        Self(profile:.excel,decimalSeparator:.recommended(for:locale),includeProvenance:false)
    }
    func validated() throws -> Self {
        guard (profile != .standard || decimalSeparator == .dot),
              decimalPlaces.map({ (0...15).contains($0) }) != false else { throw CSVExportError.invalidOptions }
        return self
    }
    func field(_ value: String) -> String {
        if value.contains(delimiter) || value.contains("\"") || value.contains("\n") || value.contains("\r") {
            return "\""+value.replacingOccurrences(of:"\"",with:"\"\"")+"\""
        }
        return value
    }
    func number(_ value: Double?) throws -> String {
        guard let value else { return "" }
        guard value.isFinite else { throw CSVExportError.nonfiniteValue }
        var text: String
        if let decimalPlaces {
            text = String(format:"%.*f",locale:Locale(identifier:"en_US_POSIX"),decimalPlaces,value)
            if Double(text) == 0, text.hasPrefix("-") { text.removeFirst() }
        } else { text = String(value) }
        return decimalSeparator == .comma ? text.replacingOccurrences(of:".",with:",") : text
    }
}

enum CSVExportError: LocalizedError {
    case invalidOptions, nonfiniteValue
    var errorDescription: String? {
        switch self {
        case .invalidOptions: "Standard CSV requires decimal points. Rounding must be between 0 and 15 decimal places."
        case .nonfiniteValue: "A measurement exceeds the numerical range supported by CSV export."
        }
    }
}
