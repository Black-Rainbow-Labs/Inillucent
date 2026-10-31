import Foundation
import XCTest
@testable import Inillucent

/// Runs `drivers/conformance/suite.json` through the Swift binding, on the iOS simulator under
/// `xcodebuild test` and on the Mac under `swift test`.
///
/// It asserts what `drivers/bindings/python/run_conformance.py` and the Rust driver's
/// `tests/conformance.rs` assert, from the same file, copied into the test bundle by `run-tests.sh`.
/// This binding holds a connection, so it lacks no capability and must run every case: the test
/// fails when it ran fewer cases than the suite holds. It writes the record every runner writes, as
/// `ios.json` on the simulator and `swift-macos.json` on the Mac, into `INILLUCENT_RECORD_DIR` when
/// that is set.
final class ConformanceTests: XCTestCase {
    /// Every case of the suite, then the record.
    func testTheWholeSuitePasses() throws {
        let url = try XCTUnwrap(Bundle.module.url(forResource: "suite", withExtension: "json"), "suite.json is not in the test bundle; run-tests.sh copies it")
        let suite = try JSONDecoder().decode(Suite.self, from: Data(contentsOf: url))
        var failures: [String] = []
        var ran: [String] = []
        for testCase in suite.cases {
            for problem in try run(testCase) { failures.append("\(testCase.name): \(problem)") }
            ran.append(testCase.name)
        }
        try writeRecord(ran: ran, failures: failures)
        XCTAssertTrue(failures.isEmpty, "the suite did not hold:\n  " + failures.joined(separator: "\n  "))
        XCTAssertEqual(ran.count, suite.cases.count)
        XCTAssertGreaterThanOrEqual(ran.count, 30, "the read is wrong rather than the suite short")
    }

    /// Runs one case on a scratch database and returns what did not hold.
    ///
    /// A case marked `"connection": "per_call"` asks for a fresh connection handle for every
    /// statement, all continuing one session. The C library does that on every call made through one
    /// `inillucent_conn`, so one `Connection` for the whole case satisfies it, as it does for the
    /// Python runner.
    /// - Parameter testCase: the case
    private func run(_ testCase: SuiteCase) throws -> [String] {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("conformance-\(testCase.name)-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        var wrong: [String] = []
        let database = try Database(url: folder.appendingPathComponent("case.rdb"))
        do {
            let connection = try database.session()
            for sql in testCase.setup ?? [] {
                do { try connection.execute(sql) } catch let why as InillucentError {
                    wrong.append("the setup statement `\(sql)` was refused: \(why)")
                }
            }
            if wrong.isEmpty {
                for step in testCase.steps ?? [] {
                    for problem in run(step, on: connection) { wrong.append("`\(step.sql)`: \(problem)") }
                }
            }
            connection.close()
        }
        try database.close()
        return wrong
    }

    /// Runs one step and returns what did not hold.
    /// - Parameters:
    ///   - step: the step
    ///   - connection: the connection
    private func run(_ step: Step, on connection: Connection) -> [String] {
        var said: [String] = []
        do {
            let rows = try connection.query(step.sql, (step.params ?? []).map(\.value), limit: step.limit)
            checkSuccess(step, rows, &said)
        } catch let failure as InillucentError {
            checkFailure(step, failure, &said)
        } catch {
            said.append("it threw something that is not an InillucentError: \(error)")
        }
        return said
    }

    /// Checks a step that was expected to succeed.
    /// - Parameters:
    ///   - step: the step
    ///   - rows: what it produced
    ///   - said: where problems go
    private func checkSuccess(_ step: Step, _ rows: Rows, _ said: inout [String]) {
        if let status = step.status {
            said.append("expected it to fail with `\(status)` and it succeeded")
            return
        }
        if let columns = step.columns, columns != rows.columns { said.append("columns are \(rows.columns) and should be \(columns)") }
        if let want = step.rows {
            if want.count != rows.count {
                said.append("there are \(rows.count) rows and there should be \(want.count)")
            } else {
                for (r, row) in want.enumerated() {
                    let got = rows.rows[r]
                    if row.count != got.count {
                        said.append("row \(r) has \(got.count) cells and should have \(row.count)")
                        continue
                    }
                    for (c, cell) in row.enumerated() where cell.value != got[c] {
                        said.append("row \(r) column \(c) is \(got[c]) and should be \(cell.value)")
                    }
                }
            }
        }
        if case .some(let want) = step.affected, rows.affected != want { said.append("affected is \(String(describing: rows.affected)) and should be \(String(describing: want))") }
        if let total = step.total, rows.total != total { said.append("total is \(rows.total) and should be \(total)") }
        if let more = step.more, rows.more != more { said.append("more is \(rows.more) and should be \(more)") }
    }

    /// Checks a step that was expected to fail.
    /// - Parameters:
    ///   - step: the step
    ///   - failure: how it failed
    ///   - said: where problems go
    private func checkFailure(_ step: Step, _ failure: InillucentError, _ said: inout [String]) {
        guard let want = step.status else {
            said.append("it was expected to succeed and it failed: \(failure)")
            return
        }
        if failure.status.wireName != want { said.append("it failed with `\(failure.status.wireName)` and should have failed with `\(want)` - \(failure)") }
        if let part = step.messageContains, !failure.message.contains(part) { said.append("the message is \"\(failure.message)\" and should hold \"\(part)\"") }
        if let part = step.featureContains {
            if let feature = failure.feature, !feature.isEmpty {
                if !feature.contains(part) { said.append("it named \"\(feature)\" and should have named something holding \"\(part)\"") }
            } else {
                said.append("it named no construct, and an unsupported refusal has to name one")
            }
        }
        if failure.isUnsupported && (failure.feature ?? "").isEmpty { said.append("an unsupported refusal must carry a feature") }
    }

    /// Writes the record every conformance runner writes, for the tooling guard to read.
    /// - Parameters:
    ///   - ran: the cases this run graded
    ///   - failures: what did not hold
    private func writeRecord(ran: [String], failures: [String]) throws {
        guard let directory = ProcessInfo.processInfo.environment["INILLUCENT_RECORD_DIR"], !directory.isEmpty else { return }
        #if os(iOS)
        let language = "ios"
        #else
        let language = "swift-macos"
        #endif
        let record: [String: Any] = ["language": language, "lacks": [String](), "ran": ran, "skipped": [String](), "failures": failures]
        let data = try JSONSerialization.data(withJSONObject: record, options: [.prettyPrinted, .sortedKeys])
        try FileManager.default.createDirectory(atPath: directory, withIntermediateDirectories: true)
        try data.write(to: URL(fileURLWithPath: directory).appendingPathComponent("\(language).json"))
    }
}

/// The parts of the suite file this runner reads.
struct Suite: Decodable {
    let cases: [SuiteCase]
}

/// One case.
struct SuiteCase: Decodable {
    let name: String
    let setup: [String]?
    let steps: [Step]?
}

/// One step. Its keys are optional, and a step asserts only the keys it carries.
struct Step: Decodable {
    let sql: String
    let params: [SuiteValue]?
    let limit: UInt64?
    let columns: [String]?
    let rows: [[SuiteValue]]?
    /// Absent: not asserted. `.some(nil)`: asserted to be a query. `.some(n)`: n rows changed.
    let affected: Int64??
    let total: Int?
    let more: Bool?
    let status: String?
    let messageContains: String?
    let featureContains: String?

    enum CodingKeys: String, CodingKey {
        case sql, params, limit, columns, rows, affected, total, more, status
        case messageContains = "message_contains"
        case featureContains = "feature_contains"
    }

    /// Reads a step, keeping an absent `affected` apart from a null one.
    /// - Parameter decoder: the decoder
    init(from decoder: Decoder) throws {
        let keys = try decoder.container(keyedBy: CodingKeys.self)
        sql = try keys.decode(String.self, forKey: .sql)
        params = try keys.decodeIfPresent([SuiteValue].self, forKey: .params)
        limit = try keys.decodeIfPresent(UInt64.self, forKey: .limit)
        columns = try keys.decodeIfPresent([String].self, forKey: .columns)
        rows = try keys.decodeIfPresent([[SuiteValue]].self, forKey: .rows)
        if keys.contains(.affected) {
            if try keys.decodeNil(forKey: .affected) {
                affected = .some(nil)
            } else {
                affected = .some(try keys.decode(Int64.self, forKey: .affected))
            }
        } else {
            affected = .none
        }
        total = try keys.decodeIfPresent(Int.self, forKey: .total)
        more = try keys.decodeIfPresent(Bool.self, forKey: .more)
        status = try keys.decodeIfPresent(String.self, forKey: .status)
        messageContains = try keys.decodeIfPresent(String.self, forKey: .messageContains)
        featureContains = try keys.decodeIfPresent(String.self, forKey: .featureContains)
    }
}

/// A value in the suite's one key object form, so NULL and the empty string are never confused.
struct SuiteValue: Decodable {
    let value: Value

    enum Kind: String, CodingKey { case null, int, real, text, blob }

    /// Reads the one key the object holds.
    /// - Parameter decoder: the decoder
    init(from decoder: Decoder) throws {
        let keys = try decoder.container(keyedBy: Kind.self)
        if keys.contains(.null) {
            value = .null
        } else if keys.contains(.int) {
            value = .integer(try keys.decode(Int64.self, forKey: .int))
        } else if keys.contains(.real) {
            value = .real(try keys.decode(Double.self, forKey: .real))
        } else if keys.contains(.text) {
            value = .text(try keys.decode(String.self, forKey: .text))
        } else if keys.contains(.blob) {
            value = .blob(Data(try keys.decode([UInt8].self, forKey: .blob)))
        } else {
            throw DecodingError.dataCorrupted(.init(codingPath: decoder.codingPath, debugDescription: "the value names no kind"))
        }
    }
}
