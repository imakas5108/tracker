import XCTest
@testable import Tracker

final class UpstreamSampleTests: XCTestCase {
    func testPreviewAndTrackingAgreeAtBallTossOutTail() async throws {
        let root = URL(fileURLWithPath:#filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let url = root.appendingPathComponent("build/upstream-samples/mechanics_videos/BallTossOut.mp4")
        guard FileManager.default.fileExists(atPath:url.path) else { throw XCTSkip("User-supplied upstream sample archive is not installed.") }
        let stamps = try await MediaSource.timestamps(url:url)
        let last = try XCTUnwrap(stamps.last)
        let reader = try await SequentialFrames(url:url)
        defer { reader.cancel() }
        var final: DecodedFrame?
        var previous: DecodedFrame?
        while let frame = try reader.next() { previous = final; final = frame }
        let observed = try XCTUnwrap(final)
        let preview = try await MediaSource(url:url).image(at:last)
        XCTAssertEqual(observed.stamp.seconds,preview.stamp.seconds,accuracy:1e-9)
        let a = try observed.grayscale(), b = try preview.grayscale()
        XCTAssertEqual(a.width,b.width); XCTAssertEqual(a.height,b.height)
        // The ball has left at the final source frame; a repeated penultimate
        // image would falsely show an observable target at this location.
        XCTAssertEqual(Int(a.pixels[469*a.width+592]),Int(b.pixels[469*b.width+592]),accuracy:10)
        XCTAssertLessThan(Int(a.pixels[469*a.width+592]),70)
        let before = try XCTUnwrap(previous).grayscale()
        XCTAssertGreaterThan(Int(before.pixels[469*before.width+592]),150)
    }
}
