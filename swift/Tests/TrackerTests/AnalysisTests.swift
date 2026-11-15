import XCTest
@testable import Tracker

final class AnalysisTests: XCTestCase {
    func testCancellingQueuedJobDoesNotStarveFollowingJob() async throws {
        let capacity = TrackingCapacity()
        let first = UUID(), second = UUID(), third = UUID()
        try await capacity.acquire(first)
        let queued = Task { try await capacity.acquire(second) }
        for _ in 0..<100 { if await capacity.queuedCount > 0 { break }; await Task.yield() }
        queued.cancel()
        do { try await queued.value; XCTFail("Queued job should cancel") } catch is CancellationError {} catch { XCTFail("\(error)") }
        await capacity.release(first)
        try await capacity.acquire(third)
        await capacity.release(third)
    }
    func testUndoRestoresReplacedPointAndCalibration() {
        var history = AnalysisHistory()
        var first = AnalysisState()
        let track = PointTrack(name: "Target", points: [Measurement(time: 0, pixel: CGPoint(x: 20, y: 20))])
        first.tracks = [track]; first.selectedTrackID = track.id
        history.replace(first)
        var second = first
        second.tracks[0].points[0] = Measurement(time: 0, pixel: CGPoint(x: 30, y: 40))
        second.calibration = Calibration(first: .zero, second: CGPoint(x: 100, y: 0), length: 1, unit: "m")
        history.replace(second); history.undo()
        XCTAssertEqual(history.state, first)
        history.redo(); XCTAssertEqual(history.state, second)
        history.undo(); history.replace(AnalysisState())
        XCTAssertFalse(history.canRedo)
    }

    func testProjectRoundTripPreservesProvenanceAndRejectsInvalidData() throws {
        let point = Measurement(time: 0.08, pixel: CGPoint(x: 50, y: 70), origin: .automatic, score: 0.95,
                                frame: FrameStamp(value: 2, timescale: 25))
        let track = PointTrack(name: "Target", points: [point])
        let state = AnalysisState(tracks: [track], selectedTrackID: track.id,
                                  calibration: Calibration(first: .zero, second: CGPoint(x: 100, y: 0), length: 1, unit: "cm"))
        var file = ProjectFile(source: URL(fileURLWithPath: "/tmp/source.mov"), sourceSize: CGSize(width: 160, height: 120),
                               analysis: state, time: 0.08, settings: TrackingSettings())
        let decoded = try JSONDecoder().decode(ProjectFile.self, from: JSONEncoder().encode(file)).validated()
        XCTAssertEqual(decoded.analysis, state)
        file.version = 200; XCTAssertThrowsError(try file.validated())
        file.version = 1; file.analysis.tracks[0].points.append(point)
        XCTAssertThrowsError(try file.validated())
    }

    func testInvalidSavePreservesPreviousProjectFile() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let url = directory.appendingPathComponent("analysis.json")
        var file = ProjectFile(source: URL(fileURLWithPath: "/tmp/source.mov"), sourceSize: CGSize(width: 160, height: 120),
                               analysis: AnalysisState(), time: 0, settings: TrackingSettings())
        try file.write(url)
        let original = try Data(contentsOf: url)
        file.version = 999
        XCTAssertThrowsError(try file.write(url))
        XCTAssertEqual(try Data(contentsOf: url), original)
        XCTAssertEqual(try ProjectFile.read(url).version, 1)
    }

    func testMatcherFindsTranslationAndRejectsDistractorAmbiguity() throws {
        let settings = TrackingSettings(templateRadius: 6, searchRadius: 24, minimumScore: 0.90, minimumSeparation: 0.005)
        func image(centers: [CGPoint]) -> GrayImage {
            var pixels = [UInt8](repeating: 30, count: 100*80)
            for center in centers {
                for dy in -5...5 { for dx in -5...5 {
                    pixels[(Int(center.y)+dy)*100+Int(center.x)+dx] = ((dx+dy)%3 == 0) ? 240 : 100
                } }
            }
            return GrayImage(width: 100, height: 80, pixels: pixels)
        }
        let seed = CGPoint(x: 35, y: 35)
        let matcher = try TemplateMatcher(image: image(centers: [seed]), center: seed, settings: settings)
        let match = try matcher.match(image(centers: [CGPoint(x: 41, y: 38)]), around: seed)
        XCTAssertTrue(match.accepted)
        XCTAssertEqual(match.position!.x, 41, accuracy: 0.1)
        XCTAssertEqual(match.position!.y, 38, accuracy: 0.1)
        let ambiguous = try matcher.match(image(centers: [CGPoint(x: 25, y: 35), CGPoint(x: 48, y: 35)]), around: seed)
        XCTAssertFalse(ambiguous.accepted)
        XCTAssertEqual(ambiguous.separation, 0, accuracy: 0.0001)
        let occluded = try matcher.match(image(centers: []), around: seed)
        XCTAssertFalse(occluded.accepted)
    }

    func testMatcherRejectsUniformPatchAndChecksCancellation() async throws {
        let flat = GrayImage(width: 100, height: 100, pixels: [UInt8](repeating: 128, count: 10000))
        XCTAssertThrowsError(try TemplateMatcher(image: flat, center: CGPoint(x: 50, y: 50), settings: TrackingSettings()))
        let textured = GrayImage(width: 100, height: 100, pixels: (0..<10000).map { UInt8(($0*73)%255) })
        let matcher = try TemplateMatcher(image: textured, center: CGPoint(x: 50, y: 50), settings: TrackingSettings())
        let task = Task {
            withUnsafeCurrentTask { $0?.cancel() }
            return try matcher.match(textured, around: CGPoint(x: 50, y: 50))
        }
        do { _ = try await task.value; XCTFail("Expected cancellation") } catch is CancellationError {} catch { XCTFail("\(error)") }
    }
}
