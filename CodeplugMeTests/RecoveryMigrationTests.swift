import XCTest
@testable import CodeplugMe

/// The one-time move of the pre-rename `AnyToneMac/Recovery` slot into the
/// CodeplugMe one. Runs against temp folders only, never the real slot.
final class RecoveryMigrationTests: XCTestCase {

    private var tempDir: URL!
    private var legacy: URL!
    private var current: URL!

    override func setUpWithError() throws {
        try super.setUpWithError()
        tempDir = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("codeplugme-migration-tests-\(UUID().uuidString)")
        legacy = tempDir.appendingPathComponent("AnyToneMac/Recovery", isDirectory: true)
        current = tempDir.appendingPathComponent("CodeplugMe/Recovery", isDirectory: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: tempDir)
        try super.tearDownWithError()
    }

    /// Writes `text` to `name` inside `dir`, creating the folder first.
    private func write(_ text: String, to name: String, in dir: URL) throws {
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try Data(text.utf8).write(to: dir.appendingPathComponent(name))
    }

    func testMovesLegacyFolderWhenOnlyLegacyExists() throws {
        try write("old", to: "work.bin", in: legacy)

        Recovery.migrate(from: legacy, to: current)

        let moved = try String(contentsOf: current.appendingPathComponent("work.bin"), encoding: .utf8)
        XCTAssertEqual(moved, "old")
        XCTAssertFalse(FileManager.default.fileExists(atPath: legacy.path))
    }

    func testLeavesExistingCurrentFolderAlone() throws {
        try write("old", to: "work.bin", in: legacy)
        try write("new", to: "work.bin", in: current)

        Recovery.migrate(from: legacy, to: current)

        let kept = try String(contentsOf: current.appendingPathComponent("work.bin"), encoding: .utf8)
        XCTAssertEqual(kept, "new")
        XCTAssertTrue(FileManager.default.fileExists(atPath: legacy.path))
    }

    func testNoLegacyFolderIsANoOp() {
        Recovery.migrate(from: legacy, to: current)

        XCTAssertFalse(FileManager.default.fileExists(atPath: current.path))
    }
}
