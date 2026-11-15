import XCTest
import AVFoundation
@testable import Tracker

private actor EventLog {
    var points: [Tracker.Measurement] = []
    var stoppedAt: Double?
    var completed = false
    func record(_ event: TrackingEvent) {
        switch event {
        case .update(let batch, _, _): points += batch
        case .review(let batch, let review, _): points += batch; stoppedAt = review.frame.stamp.seconds
        case .complete(let batch, _, _): points += batch; completed = true
        }
    }
}

final class MediaIntegrationTests: XCTestCase {
    private var fixtureDirectory: URL {
        URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("build/fixtures")
    }
    private func fixture(_ name: String) throws -> URL {
        let url = fixtureDirectory.appendingPathComponent(name+".mov")
        guard FileManager.default.fileExists(atPath: url.path) else { throw XCTSkip("Run python3 scripts/make-tracking-fixtures.py first.") }
        return url
    }
    func testReaderAndPausedFrameAgreeOnOrientationAndVFRTime() async throws {
        let url = try fixture("clean")
        let timestamps = try await MediaSource.timestamps(url: url)
        XCTAssertEqual(timestamps.count, 60)
        XCTAssertEqual(timestamps[1].seconds-timestamps[0].seconds, 0.04, accuracy: 0.00001)
        XCTAssertEqual(timestamps[2].seconds-timestamps[1].seconds, 0.08, accuracy: 0.00001)
        let media = MediaSource(url: url)
        let paused = try await media.image(at: timestamps[1].seconds)
        XCTAssertEqual(paused.stamp.seconds, timestamps[1].seconds, accuracy: 0.000001)
        let stream = try await SequentialFrames(url: url)
        defer { stream.cancel() }
        _ = try stream.next()
        let sequential = try XCTUnwrap(stream.next())
        let a = try paused.grayscale(), b = try sequential.grayscale()
        XCTAssertEqual(a.width, b.width); XCTAssertEqual(a.height, b.height)
        let error = zip(a.pixels, b.pixels).reduce(0.0) { $0 + abs(Double($1.0)-Double($1.1)) } / Double(a.pixels.count)
        XCTAssertLessThan(error, 3)
        // The synthetic target is above the center: catches an accidental vertical flip.
        XCTAssertGreaterThan(a.pixels[45*a.width+31], a.pixels[75*a.width+31])
    }
    func testCleanAndNoisyVideoLocalization() async throws {
        for name in ["clean", "noise"] {
            let url = try fixture(name)
            let log = EventLog()
            let started = Date()
            try await TrackingEngine.run(url: url, seed: Measurement(time: 0, pixel: CGPoint(x: 30, y: 45)),
                                         settings: TrackingSettings(templateRadius: 8), after: nil) { await log.record($0) }
            let rows = await log.points
            let completed = await log.isComplete()
            XCTAssertTrue(completed)
            XCTAssertEqual(rows.count, 59)
            var maxError = 0.0
            for (index, point) in rows.enumerated() {
                let frame = index+1
                maxError = max(maxError, hypot(point.pixel.x-CGFloat(30+frame), point.pixel.y-CGFloat(45+frame/4)))
            }
            XCTAssertLessThan(maxError, 0.6)
            print("TRACKER_BENCH \(name): frames=\(rows.count) elapsed_s=\(Date().timeIntervalSince(started)) max_error_px=\(maxError)")
        }
    }
    func testOcclusionAndDistractorRequireReview() async throws {
        for name in ["occlusion", "distractor"] {
            let log = EventLog()
            try await TrackingEngine.run(url: fixture(name), seed: Measurement(time: 0, pixel: CGPoint(x: 30, y: 45)),
                                         settings: TrackingSettings(templateRadius: 8), after: nil) { await log.record($0) }
            let stopped = await log.reviewTime()
            XCTAssertNotNil(stopped)
            XCTAssertEqual(stopped ?? -1, 1.2, accuracy: 0.0001)
            let count = await log.count()
            XCTAssertEqual(count, 19)
        }
    }
    @MainActor
    func testSessionSeekBlocksMarkingAndUndoRestoresReplacement() async throws {
        let session = VideoSession(url: try fixture("clean"))
        defer { session.close() }
        for _ in 0..<300 {
            if !session.loading && !session.isSeeking { break }
            try await Task.sleep(for: .milliseconds(20))
        }
        XCTAssertNil(session.error); XCTAssertTrue(session.canMark)
        session.addTrack(); session.mark(CGPoint(x: 30, y: 45))
        let original = session.points
        session.mark(CGPoint(x: 31, y: 46)); session.undo()
        XCTAssertEqual(session.points, original)
        session.seek(0.1)
        XCTAssertFalse(session.canMark)
        session.mark(CGPoint(x: 99, y: 99))
        XCTAssertEqual(session.points, original)
        session.seek(0.2); session.seek(0.04)
        for _ in 0..<300 { if !session.isSeeking { break }; try await Task.sleep(for: .milliseconds(20)) }
        XCTAssertEqual(session.displayedFrame?.stamp.seconds ?? -1, 0.04, accuracy: 0.000001)
    }

    @MainActor
    func testProjectRestoresSourceFrameAndStableTrackIDs() async throws {
        let source = try fixture("clean")
        let track = PointTrack(name: "Saved target", points: [Measurement(time: 0.04, pixel: CGPoint(x: 31, y: 45), frame: FrameStamp(value: 1, timescale: 25))])
        let state = AnalysisState(tracks: [track], selectedTrackID: track.id,
                                  calibration: Calibration(first: .zero, second: CGPoint(x: 100, y: 0), length: 1, unit: "m"))
        let project = ProjectFile(source: source, sourceSize: CGSize(width: 160, height: 120), analysis: state, time: 0.04, settings: TrackingSettings())
        let session = VideoSession(url: source, project: project)
        defer { session.close() }
        for _ in 0..<300 { if !session.loading && !session.isSeeking { break }; try await Task.sleep(for: .milliseconds(20)) }
        XCTAssertNil(session.error)
        XCTAssertEqual(session.analysis, state.scientificState)
        XCTAssertEqual(session.selectedTrackID, track.id)
        XCTAssertEqual(session.displayedFrame?.stamp.seconds ?? -1, 0.04, accuracy: 0.000001)
        XCTAssertFalse(session.isDirty)
    }
}

private extension EventLog {
    func isComplete() -> Bool { completed }
    func reviewTime() -> Double? { stoppedAt }
    func count() -> Int { points.count }
}
