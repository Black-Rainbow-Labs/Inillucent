import Foundation
import InillucentFFI

/// A capability's support level.
public enum Support: Int32, Sendable {
    case no = 0
    case yes = 1
    /// Yes, with the limit the note names.
    case partial = -1
    /// No capability by that name in this build. Treat it as no: a capability nobody declared was
    /// never checked.
    case unknown = -2
}

/// One row of the engine's capability table.
public struct Capability: Sendable, Equatable {
    /// The capability, such as `cancel` or `fts5`.
    public let name: String
    /// Whether the engine does it.
    public let support: Support
    /// A sentence about it; read it when `support` is `.partial`.
    public let note: String
}

/// The library itself: its version and its capability table.
public enum Inillucent {
    /// The ABI major version this binding was written against.
    public static let abiMajor = 1

    /// The C library's ABI version as `major.minor.patch`.
    public static var abiVersion: String {
        let reported = inillucent_abi_version()
        return "\(reported / 1_000_000).\((reported / 1000) % 1000).\(reported % 1000)"
    }

    /// What the driver calls itself, such as `inillucent-driver 2.9.0`.
    public static var version: String { cText(inillucent_version()) ?? "" }

    /// Every capability the engine declares, in the engine's order.
    public static var capabilities: [Capability] {
        (0..<inillucent_capability_count()).map { nth in
            var name: UnsafePointer<CChar>?
            var note: UnsafePointer<CChar>?
            var state: Int32 = Support.unknown.rawValue
            _ = inillucent_capability(nth, &name, &state, &note)
            return Capability(name: cText(name) ?? "", support: Support(rawValue: state) ?? .unknown, note: cText(note) ?? "")
        }
    }

    /// Looks one capability up by name.
    /// - Parameter name: the capability
    public static func supports(_ name: String) -> Support {
        if name.utf8.contains(0) { return .unknown }
        return Support(rawValue: inillucent_supports(name)) ?? .unknown
    }
}
