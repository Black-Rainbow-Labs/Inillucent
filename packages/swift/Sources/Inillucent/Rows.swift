import Foundation
import InillucentFFI

/// Everything one statement produced. The engine materialises a result, so every count was taken
/// rather than estimated.
public struct Rows: Sequence {
    /// The result's column names, in order.
    public let columns: [String]
    /// Each column's declared type, or "" for an expression, which has none.
    public let types: [String]
    /// The rows handed back, at most the limit asked for.
    public let rows: [[Value]]
    /// How many rows the statement produced, exactly, whatever the limit.
    public let total: Int
    /// Whether the limit left rows out.
    public let more: Bool
    /// Rows changed, or nil for a statement that changed nothing (a query).
    public let affected: Int64?
    /// How long the engine took, in microseconds.
    public let elapsedMicros: UInt64
    /// A one line summary such as "SELECT 27".
    public let tag: String

    /// How many rows were handed back.
    public var count: Int { rows.count }

    public func makeIterator() -> IndexingIterator<[[Value]]> { rows.makeIterator() }

    /// Reads a C result into Swift values and frees it. The result is materialised, so every pointer
    /// read here is stable until the free at the end.
    /// @param handle - the result, which this frees
    init(taking handle: OpaquePointer) {
        defer { inillucent_rows_free(handle) }
        let width = inillucent_rows_column_count(handle)
        let height = inillucent_rows_count(handle)
        columns = (0..<width).map { cText(inillucent_rows_column_name(handle, $0)) ?? "" }
        types = (0..<width).map { cText(inillucent_rows_column_type(handle, $0)) ?? "" }
        var read: [[Value]] = []
        read.reserveCapacity(height)
        for r in 0..<height {
            read.append((0..<width).map { Rows.cell(handle, r, $0) })
        }
        rows = read
        total = inillucent_rows_total(handle)
        more = inillucent_rows_more(handle) != 0
        let changed = inillucent_rows_affected(handle)
        affected = changed >= 0 ? changed : nil
        elapsedMicros = inillucent_rows_elapsed_us(handle)
        tag = cText(inillucent_rows_tag(handle)) ?? ""
    }

    /// One cell as a Swift value.
    ///
    /// Text is decoded from the bytes and their length, never as a C string, because text may hold a
    /// NUL byte and a C string would end there.
    /// @param handle - the result
    /// @param row - the row
    /// @param column - the column
    private static func cell(_ handle: OpaquePointer, _ row: Int, _ column: Int) -> Value {
        switch inillucent_value_type(handle, row, column) {
        case INILLUCENT_INTEGER:
            return .integer(inillucent_value_int(handle, row, column))
        case INILLUCENT_REAL:
            return .real(inillucent_value_real(handle, row, column))
        case INILLUCENT_TEXT:
            var len = 0
            guard let bytes = inillucent_value_bytes(handle, row, column, &len) else { return .text("") }
            return .text(String(decoding: UnsafeBufferPointer(start: bytes, count: len), as: UTF8.self))
        case INILLUCENT_BLOB:
            var len = 0
            guard let bytes = inillucent_value_bytes(handle, row, column, &len) else { return .blob(Data()) }
            return .blob(Data(bytes: bytes, count: len))
        default:
            return .null
        }
    }
}
