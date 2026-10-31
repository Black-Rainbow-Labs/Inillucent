import Foundation
import InillucentFFI

/// Why a call failed, by the names the conformance suite and every other binding use.
public enum Status: Int32, Sendable, CustomStringConvertible {
    case ok = 0
    case unsupported = 1
    case syntax = 2
    case notFound = 3
    case constraint = 4
    case readonly = 5
    case busy = 6
    case interrupted = 7
    case corrupt = 8
    case io = 9
    case full = 10
    case tooBig = 11
    case invalidState = 12
    case `internal` = 13

    /// The status for a C status number. A number this build does not know is `.internal`, since a
    /// library newer than the binding is the only way to get one.
    /// @param code - the number
    init(code: Int32) { self = Status(rawValue: code) ?? .internal }

    /// The name as `drivers/conformance/suite.json` writes it.
    public var wireName: String {
        switch self {
        case .ok: return "ok"
        case .unsupported: return "unsupported"
        case .syntax: return "syntax"
        case .notFound: return "not_found"
        case .constraint: return "constraint"
        case .readonly: return "readonly"
        case .busy: return "busy"
        case .interrupted: return "interrupted"
        case .corrupt: return "corrupt"
        case .io: return "io"
        case .full: return "full"
        case .tooBig: return "too_big"
        case .invalidState: return "invalid_state"
        case .internal: return "internal"
        }
    }

    public var description: String { wireName }
}

/// A call the engine refused.
///
/// Check `isUnsupported` first: the engine refuses what it has not built with a status of its own,
/// and an application should be able to say "not built yet" rather than "wrong".
public struct InillucentError: Error, CustomStringConvertible {
    /// Why it failed.
    public let status: Status
    /// The engine's own message, safe to show a person.
    public let message: String
    /// The construct an `unsupported` refusal names, otherwise nil.
    public let feature: String?
    /// Diagnostic text, only when the database was opened with diagnostics. It may hold a path or a
    /// bound value, so do not show it to a person.
    public let detail: String?
    /// The byte offset into the statement, or nil.
    public let offset: Int?
    /// The rows the statement produced before it failed, or nil.
    public let partialRows: Rows?

    /// Whether the engine refused because it has not built the construct.
    public var isUnsupported: Bool { status == .unsupported }

    public var description: String {
        let said = "\(message) [\(status.wireName)]"
        return offset.map { "\(said) at byte \($0)" } ?? said
    }

    /// Builds an error from its parts.
    /// - Parameters:
    ///   - status: why
    ///   - message: what happened
    ///   - feature: the construct an `unsupported` refusal names
    ///   - detail: diagnostic text
    ///   - offset: the byte offset into the statement
    ///   - partialRows: the rows produced before the failure
    init(status: Status, message: String, feature: String? = nil, detail: String? = nil, offset: Int? = nil, partialRows: Rows? = nil) {
        self.status = status
        self.message = message
        self.feature = feature
        self.detail = detail
        self.offset = offset
        self.partialRows = partialRows
    }

    /// Reads and frees the error a C call wrote, taking any rows it carries.
    /// - Parameters:
    ///   - code: the status the call returned
    ///   - error: the error, or nil when the call writes none
    init(status code: Int32, error: OpaquePointer?) {
        guard let error else {
            let status = Status(code: code)
            self.init(status: status, message: "the call failed with \(status.wireName)")
            return
        }
        let status = Status(code: inillucent_error_status(error))
        let at = inillucent_error_offset(error)
        self.init(
            status: status,
            message: cText(inillucent_error_message(error)) ?? "the call failed with \(status.wireName)",
            feature: cText(inillucent_error_feature(error)),
            detail: cText(inillucent_error_detail(error)),
            offset: at >= 0 ? Int(at) : nil,
            partialRows: inillucent_error_take_rows(error).map { Rows(taking: $0) }
        )
        inillucent_error_free(error)
    }
}

/// Decodes a C string the library owns, or nil for a null pointer.
/// @param pointer - the string
func cText(_ pointer: UnsafePointer<CChar>?) -> String? {
    pointer.map { String(cString: $0) }
}

/// Throws when a C call did not return OK.
/// @param status - what it returned
/// @param error - the error it wrote
func check(_ status: Int32, _ error: OpaquePointer?) throws {
    if status != INILLUCENT_OK { throw InillucentError(status: status, error: error) }
}

/// Refuses a string the library reads as a C string when it holds a NUL, which would end it early
/// and run something other than what was written.
/// @param text - the string
/// @param what - what it is, for the message
func refuseNul(_ text: String, _ what: String) throws {
    if text.utf8.contains(0) {
        throw InillucentError(status: .invalidState, message: "\(what) holds a NUL character, which the C library cannot read")
    }
}
