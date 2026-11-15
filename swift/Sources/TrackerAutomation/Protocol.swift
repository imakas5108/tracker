import Foundation

public enum JSONValue: Codable, Sendable, Equatable {
    case null, bool(Bool), integer(Int64), number(Double), string(String), array([JSONValue]), object([String: JSONValue])
    public init(from decoder: Decoder) throws {
        let value = try decoder.singleValueContainer()
        if value.decodeNil() { self = .null }
        else if let v = try? value.decode(Bool.self) { self = .bool(v) }
        else if let v = try? value.decode(Int64.self) { self = .integer(v) }
        else if let v = try? value.decode(Double.self) { self = .number(v) }
        else if let v = try? value.decode(String.self) { self = .string(v) }
        else if let v = try? value.decode([JSONValue].self) { self = .array(v) }
        else { self = .object(try value.decode([String: JSONValue].self)) }
    }
    public func encode(to encoder: Encoder) throws {
        var value = encoder.singleValueContainer()
        switch self {
        case .null: try value.encodeNil()
        case .bool(let v): try value.encode(v)
        case .integer(let v): try value.encode(v)
        case .number(let v): try value.encode(v)
        case .string(let v): try value.encode(v)
        case .array(let v): try value.encode(v)
        case .object(let v): try value.encode(v)
        }
    }
    public subscript(_ key: String) -> JSONValue { if case .object(let v) = self { return v[key] ?? .null }; return .null }
    public var string: String? { if case .string(let v) = self { return v }; return nil }
    public var number: Double? {
        switch self { case .number(let v): return v; case .integer(let v): return Double(v); default: return nil }
    }
    public var bool: Bool? { if case .bool(let v) = self { return v }; return nil }
    public var object: [String: JSONValue]? { if case .object(let v) = self { return v }; return nil }
    public var array: [JSONValue]? { if case .array(let v) = self { return v }; return nil }
    public static func from<T: Encodable>(_ value: T) throws -> Self { try JSONDecoder().decode(Self.self, from: JSONEncoder().encode(value)) }
    public func decode<T: Decodable>(_ type: T.Type) throws -> T { try JSONDecoder().decode(type, from: JSONEncoder().encode(self)) }
    public func text() throws -> String {
        let encoder = JSONEncoder(); encoder.outputFormatting = [.sortedKeys]
        return String(decoding: try encoder.encode(self), as: UTF8.self)
    }
}

public struct AutomationError: Error, Codable, Sendable, LocalizedError, Equatable {
    public let code: String
    public let message: String
    public var errorDescription: String? { message }
    public init(_ code: String, _ message: String) { self.code = code; self.message = message }
}

public struct AutomationRequest: Codable, Sendable {
    public var version: Int
    public var clientID: UUID
    public var command: String
    public var arguments: [String: JSONValue]
    public init(command: String, arguments: [String: JSONValue] = [:], clientID: UUID, version: Int = 1) {
        self.version = version; self.command = command; self.arguments = arguments; self.clientID = clientID
    }
}

public struct AutomationResponse: Codable, Sendable {
    public var version = 1
    public var result: JSONValue?
    public var error: AutomationError?
    public init(result: JSONValue) { self.result = result }
    public init(error: AutomationError) { self.error = error }
}

public extension Dictionary where Key == String, Value == JSONValue {
    func requiredString(_ key: String) throws -> String {
        guard let value = self[key]?.string, !value.isEmpty else { throw AutomationError("invalid_argument", "\(key) must be a nonempty string.") }; return value
    }
    func requiredNumber(_ key: String) throws -> Double {
        guard let value = self[key]?.number, value.isFinite else { throw AutomationError("invalid_argument", "\(key) must be a finite number.") }; return value
    }
    func requiredInt(_ key: String) throws -> Int {
        let value = try requiredNumber(key)
        guard value.rounded() == value, value >= 0, value < Double(Int.max) else { throw AutomationError("invalid_argument", "\(key) must be a nonnegative integer.") }; return Int(value)
    }
    func requiredID(_ key: String) throws -> UUID {
        guard let id = UUID(uuidString: try requiredString(key)) else { throw AutomationError("invalid_argument", "\(key) must be a UUID.") }; return id
    }
}
