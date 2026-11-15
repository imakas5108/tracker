import XCTest
@testable import Tracker

final class ClipIntegrationTests: XCTestCase {
    private func fixture(_ name: String = "clean") throws -> URL {
        let url = URL(fileURLWithPath:#filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("build/fixtures/\(name).mov")
        guard FileManager.default.fileExists(atPath:url.path) else { throw XCTSkip("Run python3 scripts/make-tracking-fixtures.py.") }
        return url
    }
    @MainActor private func ready(_ s: VideoSession) async throws {
        for _ in 0..<400 {
            if !s.loading && !s.isSeeking { break }
            try await Task.sleep(for:.milliseconds(20))
        }
        try s.requireReady()
        XCTAssertNotNil(s.displayedFrame)
    }
    @MainActor func testSelectionPreservesObservationsHistoryAndVersionedSave() async throws {
        let s = VideoSession(url:try fixture()); defer { s.close() }
        try await ready(s)
        let track = s.createTrack(name:"All frames")
        let stamps = s.frameTimes
        s.edit { state in
            state.tracks[0].points = stamps.map { Measurement(time:$0.seconds,pixel:CGPoint(x:30,y:45),frame:$0) }
        }
        try s.setClip(start:3,end:12,step:2); try await ready(s)
        XCTAssertEqual(s.points.map(\.frame),[3,5,7,9,11].map { stamps[$0] })
        XCTAssertEqual(try s.requireTrack(track.id).points.count,60)
        XCTAssertEqual(s.time,stamps[3].seconds,accuracy:1e-9)
        XCTAssertTrue(s.isDirty)
        let revision = s.revision
        try s.setClip(start:3,end:12,step:2)
        XCTAssertEqual(s.revision,revision)
        let snapshot = try s.snapshot().validated()
        XCTAssertEqual(snapshot.version,4)
        XCTAssertEqual(snapshot.analysis.tracks[0].points.count,60)
        s.undo(); try await ready(s)
        XCTAssertNil(s.analysis.clip); XCTAssertEqual(s.points.count,60)
        s.redo(); try await ready(s)
        XCTAssertEqual(s.points.count,5)
        s.seek(stamps[10].seconds); try await ready(s)
        XCTAssertEqual(s.displayedFrame?.stamp,stamps[9])
        s.step(Int.max); try await ready(s)
        XCTAssertEqual(s.displayedFrame?.stamp,stamps[11])
        s.step(Int.min); try await ready(s)
        XCTAssertEqual(s.displayedFrame?.stamp,stamps[3])
        let restored = VideoSession(url:s.url,project:snapshot); defer { restored.close() }
        try await ready(restored)
        XCTAssertEqual(restored.clip,s.clip)
        XCTAssertEqual(restored.points.count,5)
        XCTAssertFalse(restored.isDirty)
        try s.resetClip(); try await ready(s)
        XCTAssertEqual(s.points.count,60)
        XCTAssertEqual(s.snapshot().version,2)
    }
    @MainActor func testTrackerOnlyMatchesSelectedStepsAndStopsAtLastSample() async throws {
        let s = VideoSession(url:try fixture()); defer { s.close() }
        try await ready(s)
        try s.configure(TrackingSettings(templateRadius:8))
        try s.setClip(start:3,end:12,step:2); try await ready(s)
        let track = s.createTrack(name:"Selected steps")
        try s.recordPoint(CGPoint(x:33,y:45),trackID:track.id,time:s.frameTimes[3].seconds)
        let job = try s.beginTracking(trackID:track.id)
        for _ in 0..<400 { if !s.isTracking { break }; try await Task.sleep(for:.milliseconds(20)) }
        XCTAssertEqual(s.trackingJobs[job]?.state,"Completed")
        XCTAssertEqual(s.trackingJobs[job]?.processed,4)
        XCTAssertEqual(s.points.map(\.frame),[3,5,7,9,11].map { s.frameTimes[$0] })
        XCTAssertEqual(s.trackingJobs[job]?.clip?.endFrame,12)
        XCTAssertEqual(s.points.last?.pixel.x ?? -1,41,accuracy:0.6)
        try s.setClip(start:3,end:3,step:1); try await ready(s)
        s.togglePlayback(); XCTAssertFalse(s.playing)
        let single = try s.beginTracking(trackID:track.id)
        for _ in 0..<400 { if !s.isTracking { break }; try await Task.sleep(for:.milliseconds(20)) }
        XCTAssertEqual(s.trackingJobs[single]?.processed,0)
        XCTAssertEqual(s.trackingJobs[single]?.state,"Completed")
    }
    @MainActor func testClipChangeCancelsQueuedJobAndCannotReceiveLatePoints() async throws {
        let s = VideoSession(url:try fixture()); defer { s.close() }
        try await ready(s)
        let track = s.createTrack(name:"Queued")
        try s.recordPoint(CGPoint(x:30,y:45),trackID:track.id,time:0)
        let blocker = UUID(); try await TrackingCapacity.shared.acquire(blocker)
        let job = try s.beginTracking(trackID:track.id)
        try s.setClip(start:3,end:12,step:2)
        XCTAssertFalse(s.isTracking); XCTAssertNil(s.review)
        XCTAssertEqual(s.trackingJobs[job]?.state,"Stopped")
        XCTAssertTrue(s.trackingJobs[job]?.clip?.isFullSource == true)
        await TrackingCapacity.shared.release(blocker)
        let follower = UUID(); try await TrackingCapacity.shared.acquire(follower)
        await TrackingCapacity.shared.release(follower)
        try await ready(s)
        XCTAssertEqual(s.tracks[0].points.count,1)
        XCTAssertEqual(s.points.count,0)
    }
    @MainActor func testSampledPlaybackAndClipChangeKeepViewerInsideSelection() async throws {
        let s = VideoSession(url:try fixture()); defer { s.close() }
        try await ready(s)
        try s.setClip(start:3,end:12,step:2); try await ready(s)
        s.togglePlayback()
        for _ in 0..<200 {
            if !s.playing { break }
            try await Task.sleep(for:.milliseconds(20))
            if let stamp = s.displayedFrame?.stamp { XCTAssertTrue(s.isSelected(stamp)) }
        }
        XCTAssertFalse(s.playing)
        XCTAssertEqual(s.displayedFrame?.stamp,s.frameTimes[11])
        s.seek(s.frameTimes[3].seconds); try await ready(s); s.togglePlayback()
        try s.setClip(start:20,end:22,step:1); try await ready(s)
        XCTAssertFalse(s.playing)
        XCTAssertEqual(s.displayedFrame?.stamp,s.frameTimes[20])
        try await Task.sleep(for:.milliseconds(200))
        XCTAssertEqual(s.displayedFrame?.stamp,s.frameTimes[20])
        XCTAssertEqual(s.player.currentItem?.forwardPlaybackEndTime,s.frameTimes[23].cmTime)
    }
    @MainActor func testClipChangeClearsUncertainReviewWithoutRecordingCandidate() async throws {
        let s = VideoSession(url:try fixture("occlusion")); defer { s.close() }
        try await ready(s)
        try s.configure(TrackingSettings(templateRadius:8))
        let track = s.createTrack(name:"Occlusion")
        try s.recordPoint(CGPoint(x:30,y:45),trackID:track.id,time:0)
        let job = try s.beginTracking(trackID:track.id)
        for _ in 0..<400 { if !s.isTracking { break }; try await Task.sleep(for:.milliseconds(20)) }
        XCTAssertNotNil(s.review)
        let retained = s.tracks[0].points
        XCTAssertEqual(retained.count,20)
        try s.setClip(start:25,end:30,step:1); try await ready(s)
        XCTAssertNil(s.review)
        XCTAssertEqual(s.trackingJobs[job]?.state,"Stopped")
        s.acceptCandidate()
        XCTAssertEqual(s.tracks[0].points,retained)
    }
    @MainActor func testNativePlaybackStopsOnInclusiveEndFrame() async throws {
        let s = VideoSession(url:try fixture()); defer { s.close() }
        try await ready(s)
        try s.setClip(start:3,end:5,step:1); try await ready(s)
        s.togglePlayback()
        for _ in 0..<200 { if !s.playing { break }; try await Task.sleep(for:.milliseconds(20)) }
        try await ready(s)
        XCTAssertFalse(s.playing)
        XCTAssertEqual(s.displayedFrame?.stamp,s.frameTimes[5])
        XCTAssertEqual(s.time,s.frameTimes[5].seconds,accuracy:1e-9)
    }
    @MainActor func testClipChangeStopsRunningMatcherAndLateResults() async throws {
        let s = VideoSession(url:try fixture()); defer { s.close() }
        try await ready(s)
        try s.configure(TrackingSettings(templateRadius:20,searchRadius:128,minimumSeparation:0))
        let track = s.createTrack(name:"Running")
        try s.recordPoint(CGPoint(x:30,y:45),trackID:track.id,time:0)
        let job = try s.beginTracking(trackID:track.id)
        for _ in 0..<100 {
            if s.jobState == "Running" { break }
            try await Task.sleep(for:.milliseconds(1))
        }
        XCTAssertEqual(s.jobState,"Running")
        try s.setClip(start:3,end:12,step:2); try await ready(s)
        XCTAssertFalse(s.isTracking)
        let retained = s.tracks[0].points
        try await Task.sleep(for:.milliseconds(150))
        XCTAssertEqual(s.tracks[0].points,retained)
        XCTAssertEqual(s.trackingJobs[job]?.state,"Stopped")
    }
}
