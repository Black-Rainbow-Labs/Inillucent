import Foundation

/// One engine value.
///
/// An enum rather than `Any?` so that NULL, an empty string and an empty blob are three different
/// values that the type system keeps apart, and so that an integer is never silently a real. It
/// conforms to the literal protocols, so `[1, "Ada", nil, 2.5, true]` is a parameter list.
public enum Value: Equatable, Hashable, Sendable {
    case null
    case integer(Int64)
    case real(Double)
    case text(String)
    case blob(Data)

    /// The value as an integer, when it is one.
    public var integer: Int64? { if case .integer(let value) = self { return value } else { return nil } }

    /// The value as a real, when it is one.
    public var real: Double? { if case .real(let value) = self { return value } else { return nil } }

    /// The value as text, when it is text.
    public var text: String? { if case .text(let value) = self { return value } else { return nil } }

    /// The value as bytes, when it is a blob.
    public var blob: Data? { if case .blob(let value) = self { return value } else { return nil } }

    /// Whether the value is NULL.
    public var isNull: Bool { self == .null }

    /// Whether two values are the same value as the engine stores them.
    ///
    /// **Text compares by its UTF-8 bytes, not by Swift's `==`.** Swift's `String ==` uses canonical
    /// equivalence, so "\u{e9}" and "e\u{301}" are equal Strings, while the engine stores them as two
    /// different values and `=` in SQL says they differ. The conformance case
    /// `composed_and_decomposed_text_stay_two_values` passed under the synthesized `==` even when
    /// both came back as the same form, which is a test that cannot fail. An integer is never equal
    /// to a real, and reals compare with IEEE equality, so -0.0 equals 0.0.
    /// - Parameters:
    ///   - lhs: one value
    ///   - rhs: the other
    public static func == (lhs: Value, rhs: Value) -> Bool {
        switch (lhs, rhs) {
        case (.null, .null): return true
        case (.integer(let a), .integer(let b)): return a == b
        case (.real(let a), .real(let b)): return a == b
        case (.text(let a), .text(let b)): return a.utf8.elementsEqual(b.utf8)
        case (.blob(let a), .blob(let b)): return a == b
        default: return false
        }
    }

    /// Hashes the value consistently with `==`.
    /// - Parameter hasher: the hasher
    public func hash(into hasher: inout Hasher) {
        switch self {
        case .null: hasher.combine(0)
        case .integer(let value): hasher.combine(1); hasher.combine(value)
        case .real(let value): hasher.combine(2); hasher.combine(value)
        case .text(let value): hasher.combine(3); hasher.combine(Array(value.utf8))
        case .blob(let value): hasher.combine(4); hasher.combine(value)
        }
    }
}

extension Value: ExpressibleByNilLiteral {
    /// NULL. @param nilLiteral - the literal
    public init(nilLiteral: ()) { self = .null }
}

extension Value: ExpressibleByIntegerLiteral {
    /// An integer. @param value - the literal
    public init(integerLiteral value: Int64) { self = .integer(value) }
}

extension Value: ExpressibleByFloatLiteral {
    /// A real. @param value - the literal
    public init(floatLiteral value: Double) { self = .real(value) }
}

extension Value: ExpressibleByStringLiteral {
    /// Text. @param value - the literal
    public init(stringLiteral value: String) { self = .text(value) }
}

extension Value: ExpressibleByBooleanLiteral {
    /// 1 or 0, the way SQLite stores a boolean. @param value - the literal
    public init(booleanLiteral value: Bool) { self = .integer(value ? 1 : 0) }
}

extension Value: CustomStringConvertible {
    /// The value as a person would read it in a failure message.
    public var description: String {
        switch self {
        case .null: return "NULL"
        case .integer(let value): return "\(value) (integer)"
        case .real(let value): return "\(value) (real)"
        case .text(let value): return "\"\(value)\" (text)"
        case .blob(let value): return "\(value.count) bytes \(Array(value))"
        }
    }
}
