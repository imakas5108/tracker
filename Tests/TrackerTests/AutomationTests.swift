import XCTest
import ImageIO
import TrackerAutomation
@testable import Tracker

final class AutomationTests: XCTestCase {
    private var source: URL { URL(fileURLWithPath:#filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent("build/fixtures/clean.mov") }
    @MainActor private func ready(_ s: VideoSession) async throws {
        for _ in 0..<500 { if !s.loading && !s.isSeeking { break }; try await Task.sleep(for:.milliseconds(20)) }
        try s.requireReady(); XCTAssertNotNil(s.displayedFrame)
    }
    @MainActor private func call(_ controller: AutomationController, _ name: String, _ args: [String:JSONValue] = [:], client: UUID = UUID()) async -> AutomationResponse {
        await controller.handle(AutomationRequest(command:"tracker_"+name,arguments:args,clientID:client))
    }
    @MainActor private func edit(_ s: VideoSession, extra: [String:JSONValue] = [:]) -> [String:JSONValue] {
        extra.merging(["session_id":.string(s.id.uuidString),"expected_revision":.number(Double(s.revision)),"request_id":.string(UUID().uuidString)]) { old,_ in old }
    }
    @MainActor
    func testExplicitSessionRevisionRetryAndUndo() async throws {
        guard FileManager.default.fileExists(atPath:source.path) else { throw XCTSkip("Generate fixtures.") }
        let workspace = Workspace()
        let control = workspace.automation; control.allowControl()
        let first = workspace.importVideo(source); try await ready(first)
        let second = VideoSession(url:source); workspace.sessions.append(second); workspace.select(second.id); try await ready(second)
        defer { first.close(); second.close() }
        let args = edit(first,extra:["name":.string("Explicit target")])
        let result = await call(control,"add_track",args)
        XCTAssertNil(result.error); XCTAssertEqual(first.tracks.count,1); XCTAssertEqual(second.tracks.count,0)
        let retry = await call(control,"add_track",args,client:UUID())
        XCTAssertEqual(retry.result,result.result); XCTAssertEqual(first.tracks.count,1)
        var conflict = args; conflict["name"] = .string("different")
        let denied = await call(control,"add_track",conflict)
        XCTAssertEqual(denied.error?.code,"request_id_conflict")
        var stale = args; stale["request_id"] = .string(UUID().uuidString)
        let old = await call(control,"add_track",stale)
        XCTAssertEqual(old.error?.code,"stale_revision")
        let track = try XCTUnwrap(first.tracks.first)
        let mark = await call(control,"mark_point",edit(first,extra:["track_id":.string(track.id.uuidString),"time":.number(0),"x":.number(30),"y":.number(45)]))
        XCTAssertNil(mark.error); XCTAssertEqual(first.tracks[0].points.count,1)
        let undone = await call(control,"undo",edit(first)); XCTAssertNil(undone.error); XCTAssertEqual(first.tracks[0].points.count,0)
        let redone = await call(control,"redo",edit(first)); XCTAssertNil(redone.error); XCTAssertEqual(first.tracks[0].points.count,1)
        control.stopControl()
        let stopped = await call(control,"list_sessions")
        XCTAssertEqual(stopped.error?.code,"control_disabled")
        let wrong = await control.handle(AutomationRequest(command:"__hello",clientID:UUID(),version:99))
        XCTAssertEqual(wrong.error?.code,"incompatible_protocol")
    }
    @MainActor
    func testInspectionDoesNotSeekAndCropMappingMatchesPixels() async throws {
        guard FileManager.default.fileExists(atPath:source.path) else { throw XCTSkip("Generate fixtures.") }
        let workspace = Workspace(), s = VideoSession(url:source)
        workspace.sessions.append(s); let c = workspace.automation; c.allowControl()
        defer { s.close() }; try await ready(s)
        let stamp = try XCTUnwrap(s.displayedFrame?.stamp)
        let response = await call(c,"get_frame",["session_id":.string(s.id.uuidString),"time":.number(0.12),"crop":.object(["x":.number(20),"y":.number(30),"width":.number(40),"height":.number(40)]),"max_edge":.number(20)])
        XCTAssertNil(response.error)
        let result = try XCTUnwrap(response.result)
        XCTAssertEqual(result["time"].number ?? -1,0.12,accuracy:0.000001)
        XCTAssertEqual(result["mapping"]["scale_x"].number,0.5)
        XCTAssertEqual(result["image_width"].number,20)
        XCTAssertEqual(s.displayedFrame?.stamp,stamp); XCTAssertEqual(s.time,0)
        let data = try XCTUnwrap(Data(base64Encoded:try XCTUnwrap(result["image_base64"].string)))
        let image = try XCTUnwrap(CGImageSourceCreateImageAtIndex(try XCTUnwrap(CGImageSourceCreateWithData(data as CFData,nil)),0,nil))
        let gray = try DecodedFrame(stamp:stamp,image:image).grayscale()
        // Target at source (32,45) maps to image (6,7.5), not its vertical reflection.
        XCTAssertGreaterThan(gray.pixels[7*20+6],gray.pixels[15*20+6])
        let analysis = AnalysisState(tracks:[PointTrack(name:"overlay",points:[Measurement(time:0,pixel:CGPoint(x:30,y:45))])])
        let overlay = try FramePreview.render(try XCTUnwrap(s.displayedFrame),analysis:analysis,revision:0,crop:nil,maxEdge:1280,overlays:true)
        let overlayData = try XCTUnwrap(Data(base64Encoded:try XCTUnwrap(overlay["image_base64"].string)))
        let oi = try XCTUnwrap(CGImageSourceCreateImageAtIndex(try XCTUnwrap(CGImageSourceCreateWithData(overlayData as CFData,nil)),0,nil))
        let bytes = try DecodedFrame(stamp:stamp,image:oi).grayscale()
        XCTAssertGreaterThan(bytes.pixels[45*160+34],bytes.pixels[75*160+34])
    }
    @MainActor
    func testQueuedCancellationDisconnectAndRetainedStatus() async throws {
        guard FileManager.default.fileExists(atPath:source.path) else { throw XCTSkip("Generate fixtures.") }
        let workspace = Workspace(), s = workspace.importVideo(source); try await ready(s)
        let c = workspace.automation; c.allowControl(); defer { s.close() }
        let track = s.createTrack(name:"queued"); try s.recordPoint(CGPoint(x:30,y:45),trackID:track.id,time:0)
        let blocker = UUID(); try await TrackingCapacity.shared.acquire(blocker)
        let started = await call(c,"start_tracking",edit(s,extra:["track_id":.string(track.id.uuidString)]))
        let id = try XCTUnwrap(started.result?["job_id"].string)
        _ = await c.handle(AutomationRequest(command:"__disconnect",clientID:UUID()))
        XCTAssertTrue(s.isTracking)
        let cancelled = await call(c,"cancel_job",["job_id":.string(id),"request_id":.string(UUID().uuidString)])
        XCTAssertNil(cancelled.error)
        await TrackingCapacity.shared.release(blocker)
        try await Task.sleep(for:.milliseconds(50))
        XCTAssertFalse(s.isTracking); XCTAssertEqual(s.points.count,1)
        let status = await call(c,"get_job",["job_id":.string(id)])
        XCTAssertEqual(status.result?["state"].string,"stopped")
        s.close()
        let retained = await call(c,"get_job",["job_id":.string(id)])
        XCTAssertEqual(retained.result?["state"].string,"stopped")
    }
    @MainActor
    func testTrackingFailureIsRetainedWithoutAnAgentModal() async throws {
        let occlusion = source.deletingLastPathComponent().appendingPathComponent("occlusion.mov")
        guard FileManager.default.fileExists(atPath:occlusion.path) else { throw XCTSkip("Generate fixtures.") }
        let session = VideoSession(url:occlusion); defer { session.close() }
        try await ready(session); session.seek(1.2); try await ready(session)
        let track = session.createTrack(name:"Insufficient detail")
        try session.recordPoint(CGPoint(x:25,y:25),trackID:track.id,time:1.2)
        let id = try session.beginTracking(trackID:track.id)
        for _ in 0..<200 { if !session.isTracking { break }; try await Task.sleep(for:.milliseconds(20)) }
        XCTAssertEqual(session.trackingJobs[id]?.state,"Failed")
        XCTAssertNotNil(session.trackingJobs[id]?.error)
        XCTAssertNotNil(session.trackingError)
        XCTAssertNil(session.exportError)
    }
    @MainActor
    func testCalibrationAndFileFailuresAreStructured() async throws {
        guard FileManager.default.fileExists(atPath:source.path) else { throw XCTSkip("Generate fixtures.") }
        let workspace = Workspace(), s = workspace.importVideo(source); try await ready(s)
        let c = workspace.automation; c.allowControl(); defer { s.close() }
        let invalid = await call(c,"set_calibration",edit(s,extra:["x1":.number(10),"y1":.number(10),"x2":.number(10),"y2":.number(10),"length":.number(1),"unit":.string("m")]))
        XCTAssertEqual(invalid.error?.code,"invalid_calibration"); XCTAssertNil(s.calibration)
        let missing = await call(c,"save_project",edit(s,extra:["path":.string("/no-such-tracker-directory/result.json")]))
        let id = try XCTUnwrap(missing.result?["job_id"].string)
        for _ in 0..<100 {
            let status = await call(c,"get_job",["job_id":.string(id)])
            if status.result?["state"].string == "failed" { XCTAssertNotNil(status.result?["error"].object); return }
            try await Task.sleep(for:.milliseconds(20))
        }
        XCTFail("File failure was not retained")
    }
}
