import XCTest
import AVFoundation
import TrackerAutomation
@testable import Tracker

final class AuditRegressionTests: XCTestCase {
    private var root: URL { URL(fileURLWithPath:#filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent() }
    private func fixture(_ variant: String) throws -> URL {
        let file = root.appendingPathComponent("build/audit/balltoss/synthetic-ball-toss-\(variant).mp4")
        guard FileManager.default.fileExists(atPath:file.path) else { throw XCTSkip("Run scripts/make-balltoss-fixtures.py") }
        return file
    }
    @MainActor private func until(_ predicate: () -> Bool) async throws {
        let deadline = Date().addingTimeInterval(15)
        while !predicate() {
            guard Date() < deadline else { throw NSError(domain:"AuditRegression",code:1,userInfo:[NSLocalizedDescriptionKey:"Timed out waiting for state"]) }
            try await Task.sleep(for:.milliseconds(10))
        }
    }
    @MainActor private func ready(_ session: VideoSession) async throws {
        try await until { !session.loading && !session.isSeeking }
        try session.requireReady(); _ = try XCTUnwrap(session.displayedFrame)
    }
    func testRationalFrameAccessAndSequentialReaderAgree() async throws {
        for variant in ["clean", "offset", "rotated"] {
            let url = try fixture(variant), stamps = try await MediaSource.timestamps(url:url)
            XCTAssertEqual(stamps.count,72)
            if variant == "offset" { XCTAssertGreaterThan(stamps[0].seconds,0) }
            let media = MediaSource(url:url), sequential = try await SequentialFrames(url:url)
            defer { sequential.cancel() }
            for stamp in stamps.prefix(12) {
                let paused = try await media.image(at:stamp)
                XCTAssertEqual(CMTimeCompare(paused.stamp.cmTime,stamp.cmTime),0)
                let frame = try XCTUnwrap(sequential.next())
                XCTAssertEqual(CMTimeCompare(frame.stamp.cmTime,stamp.cmTime),0)
                let a = try paused.grayscale(), b = try frame.grayscale()
                XCTAssertEqual(a.width,variant == "rotated" ? 1080 : 1920)
                let difference = zip(a.pixels,b.pixels).reduce(0.0) { $0 + abs(Double($1.0)-Double($1.1)) } / Double(a.pixels.count)
                XCTAssertLessThan(difference,3)
            }
        }
    }
    @MainActor func testSelectionDoesNotCancelJobOrChangeHistoryAndRevision() async throws {
        let session = VideoSession(url:try fixture("clean")); defer { session.close() }
        try await ready(session)
        let other = session.createTrack(name:"Other"), target = session.createTrack(name:"Target")
        try session.recordPoint(CGPoint(x:130,y:850),trackID:target.id,time:session.time)
        let revision = session.revision, dirty = session.isDirty
        let blocker = UUID(); try await TrackingCapacity.shared.acquire(blocker)
        let job = try session.beginTracking(trackID:target.id)
        session.selectedTrackID = other.id
        XCTAssertTrue(session.isTracking); XCTAssertEqual(session.lastTrackingID,job)
        XCTAssertEqual(session.revision,revision); XCTAssertEqual(session.isDirty,dirty)
        session.cancelTracking(); await TrackingCapacity.shared.release(blocker)
        session.undo()
        XCTAssertEqual(session.selectedTrackID,other.id)
        XCTAssertEqual(try session.requireTrack(target.id).points.count,0)
        session.redo(); XCTAssertEqual(session.selectedTrackID,other.id)
        XCTAssertEqual(try session.requireTrack(target.id).points.count,1)
        let document = session.snapshot()
        XCTAssertEqual(document.analysis.selectedTrackID,other.id)
        XCTAssertNil(session.analysis.selectedTrackID)
    }
    @MainActor func testStartDuringPlaybackIsRejectedWithoutMutation() async throws {
        let session = VideoSession(url:try fixture("clean")); defer { session.close() }
        try await ready(session)
        let target = session.createTrack(name:"Target")
        try session.recordPoint(CGPoint(x:130,y:850),trackID:target.id,time:session.time)
        let revision = session.revision
        session.togglePlayback(); XCTAssertTrue(session.playing)
        XCTAssertThrowsError(try session.beginTracking(trackID:target.id))
        XCTAssertNil(session.lastTrackingID); XCTAssertEqual(session.revision,revision)
        session.pause(); try await ready(session)
    }
    @MainActor func testTerminalOperationAndRetrySurviveSessionDeallocation() async throws {
        let workspace = Workspace(), control = workspace.automation
        control.allowControl()
        let request = AutomationRequest(command:"tracker_import_video",arguments:["path":.string(try fixture("clean").path),"request_id":.string(UUID().uuidString)],clientID:UUID())
        let accepted = await control.handle(request)
        let id = try XCTUnwrap(accepted.result?["job_id"].string)
        let operation = try XCTUnwrap(control.operations[UUID(uuidString:id)!])
        try await until { operation.state == "completed" }
        weak var reference = workspace.sessions.first
        workspace.sessions.forEach { $0.close() }; workspace.sessions.removeAll(); workspace.selectedID = nil
        try await until { reference == nil }
        XCTAssertNil(reference)
        XCTAssertEqual(try operation.snapshot()["state"].string,"completed")
        let retry = await control.handle(request)
        XCTAssertEqual(retry.result,accepted.result); XCTAssertTrue(workspace.sessions.isEmpty)
    }
    func testBatchMergePreservesCorrectionsAndOrdering() {
        let manual = Measurement(time:1,pixel:.zero)
        let reviewed = Measurement(time:2,pixel:.zero,origin:.acceptedCandidate)
        var track = PointTrack(name:"Target",points:[manual,reviewed,Measurement(time:3,pixel:.zero,origin:.automatic)])
        let batch = (1...5).map { Measurement(time:Double($0),pixel:CGPoint(x:10,y:20),origin:.automatic) }
        track.mergeAutomatic(batch)
        XCTAssertEqual(track.points.count,5); XCTAssertEqual(track.points[0],manual); XCTAssertEqual(track.points[1],reviewed)
        XCTAssertEqual(track.points[2].pixel.x,10)
        XCTAssertEqual(track.points.map(\.time),[1,2,3,4,5])
    }
}

private actor BallTossLog {
    var rows: [Tracker.Measurement] = []
    var reviewTime: Double?
    func receive(_ event: TrackingEvent) {
        switch event {
        case .update(let points, _, _), .complete(let points, _, _): rows += points
        case .review(let points, let review, _): rows += points; reviewTime = review.frame.stamp.seconds
        }
    }
}

extension AuditRegressionTests {
    func testBallTossGroundTruthAndOcclusion() async throws {
        for variant in ["clean", "occluded"] {
            let url = try fixture(variant), log = BallTossLog()
            try await TrackingEngine.run(url:url,seed:Measurement(time:0,pixel:CGPoint(x:130,y:850),frame:FrameStamp(value:0,timescale:24000)),settings:TrackingSettings(),after:nil) { await log.receive($0) }
            let rows = await log.rows, pending = await log.reviewTime
            if variant == "clean" { XCTAssertNil(pending); XCTAssertEqual(rows.count,71) }
            else { XCTAssertEqual(pending ?? -1,Double(28*1001)/24000,accuracy:0.000001); XCTAssertEqual(rows.count,27) }
            var maxError = 0.0
            for row in rows {
                let t = row.time
                let error = hypot(row.pixel.x-(130+450*t).rounded(),row.pixel.y-(850-500*t+150*t*t).rounded())
                maxError = max(maxError,error)
                XCTAssertNotNil(row.frame)
            }
            XCTAssertLessThan(maxError,1)
            print("BALLTOSS \(variant) observations=\(rows.count) max_error_px=\(maxError) pending_s=\(pending.map(String.init(describing:)) ?? "none")")
        }
    }
    @MainActor func testDisconnectLetsOwnedTrackingFinishAndTerminalRecordOutlivesSession() async throws {
        let workspace = Workspace(), control = workspace.automation; control.allowControl()
        var session: VideoSession? = workspace.importVideo(try fixture("clean")); try await ready(session!)
        let target = session!.createTrack(name:"Disconnect")
        try session!.recordPoint(CGPoint(x:130,y:850),trackID:target.id,time:0)
        let owner = UUID()
        let request = AutomationRequest(command:"tracker_start_tracking",arguments:["session_id":.string(session!.id.uuidString),"track_id":.string(target.id.uuidString),"expected_revision":.number(Double(session!.revision)),"request_id":.string(UUID().uuidString)],clientID:owner)
        let response = await control.handle(request)
        let id = UUID(uuidString:try XCTUnwrap(response.result?["job_id"].string))!
        _ = await control.handle(AutomationRequest(command:"__disconnect",clientID:owner))
        XCTAssertNil(control.clients[owner])
        try await until { !session!.isTracking }
        XCTAssertEqual(session!.trackingJobs[id]?.state,"Completed")
        XCTAssertEqual(try session!.requireTrack(target.id).points.count,72)
        weak var reference = session
        session!.close(); workspace.sessions.removeAll(); workspace.selectedID = nil; session = nil
        try await until { reference == nil }
        let retained = try XCTUnwrap(control.operations[id]).snapshot()
        XCTAssertEqual(retained["state"].string,"completed")
        XCTAssertEqual(retained["processed_frames"].number,71)
    }
}
