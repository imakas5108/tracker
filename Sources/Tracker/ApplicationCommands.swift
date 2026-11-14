import Foundation
import TrackerAutomation

struct SessionTrackingJob {
    var id: UUID
    var trackID: UUID
    var state: String
    var processed: Int
    var error: String?
}

@MainActor
extension VideoSession {
    var reviewTrackID: UUID? { lastTrackingID.flatMap { trackingJobs[$0]?.trackID } }
    func recordTrackingState() {
        guard let id = lastTrackingID else { return }
        trackingJobs[id]?.state = jobState; trackingJobs[id]?.processed = processedFrames
    }
    func requireReady() throws {
        guard !closed else { throw AutomationError("session_closed", "The video session was closed.") }
        if let error { throw AutomationError("media_error", error) }
        guard !loading else { throw AutomationError("session_loading", "Wait for the import operation to complete.") }
    }
    func requireTrack(_ id: UUID) throws -> PointTrack {
        guard let track = tracks.first(where: { $0.id == id }) else { throw AutomationError("track_not_found", "No such track in this session.") }
        return track
    }
    func validatePixel(_ point: CGPoint) throws {
        guard point.x.isFinite, point.y.isFinite, point.x >= 0, point.y >= 0, point.x < size.width, point.y < size.height else {
            throw AutomationError("invalid_coordinate", "Coordinates must lie inside the orientation-corrected source image.")
        }
    }
    @discardableResult func createTrack(name: String) -> PointTrack {
        let track = PointTrack(name: name)
        edit { $0.tracks.append(track); $0.selectedTrackID = track.id }; tool = .mark
        return track
    }
    func recordPoint(_ point: CGPoint, trackID: UUID, time: Double) throws {
        try requireReady(); _ = try requireTrack(trackID); try validatePixel(point)
        guard canMark, let frame = displayedFrame, abs(frame.stamp.seconds-time) < 0.000001 else {
            throw AutomationError("frame_not_ready", "Pause and seek the native viewer to the requested source frame before marking.")
        }
        edit { state in
            let index = state.tracks.firstIndex(where: { $0.id == trackID })!
            Self.put(Measurement(time: frame.stamp.seconds, pixel: point, frame: frame.stamp), into: &state.tracks[index])
        }
    }
    func applyCalibration(_ value: Calibration) throws {
        try requireReady(); try validatePixel(value.first); try validatePixel(value.second)
        guard value.isValid, ["m","cm","mm"].contains(value.unit) else { throw AutomationError("invalid_calibration", "Supply distinct endpoints, a positive known length and m, cm or mm.") }
        calibration = value
    }
    func configure(_ value: TrackingSettings) throws {
        try requireReady()
        guard value.isValid else { throw AutomationError("invalid_settings", TrackingError.invalidSettings.localizedDescription) }
        settings = value
    }
    func beginTracking(trackID: UUID) throws -> UUID {
        try requireReady(); let track = try requireTrack(trackID)
        guard !isTracking, !isSeeking, let stamp = displayedFrame?.stamp,
              track.points.contains(where: { abs($0.time-stamp.seconds) < 0.000001 }) else {
            throw AutomationError("invalid_seed", "Wait for the current job/seek, and mark this track on the displayed frame.")
        }
        selectedTrackID = trackID
        startTracking(trackID: trackID)
        guard let id = lastTrackingID else { throw AutomationError("job_failed", "Tracking could not start.") }
        return id
    }
    func writeProject(to destination: URL, overwrite: Bool) async throws {
        try requireReady()
        guard !saving else { throw AutomationError("write_busy", "A project save is already in progress.") }
        try validateOutput(destination)
        let document = snapshot(); setSaving(true)
        defer { setSaving(false) }
        try await Task.detached(priority: .utility) {
            let data = try JSONEncoder().encode(document.validated())
            try Task.checkCancellation()
            try AtomicOutput.write(data, to: destination, overwrite: overwrite)
        }.value
        didSave(document, at: destination)
    }
    func writeCSV(trackID: UUID, to destination: URL, overwrite: Bool) async throws {
        try requireReady(); try validateOutput(destination)
        let rows = try requireTrack(trackID).points
        let scale = calibration?.pixelsPerUnit ?? 1, height = size.height, units = unit
        try await Task.detached(priority: .utility) {
            let lines = rows.map { "\($0.time),\($0.pixel.x/scale),\((height-$0.pixel.y)/scale),\($0.origin.rawValue),\($0.score.map { String($0) } ?? "")" }
            let data = Data(("time_s,x_\(units),y_\(units),source,match_score\n"+lines.joined(separator:"\n")+"\n").utf8)
            try Task.checkCancellation()
            try AtomicOutput.write(data, to: destination, overwrite: overwrite)
        }.value
    }
    private func validateOutput(_ destination: URL) throws {
        guard destination.resolvingSymlinksInPath() != url.resolvingSymlinksInPath() else {
            throw AutomationError("invalid_destination", "Analysis output cannot replace source footage.")
        }
    }
}

@MainActor
extension Workspace {
    @discardableResult func importVideo(_ url: URL) -> VideoSession {
        if let existing = sessions.first(where: { $0.url == url }) { select(existing.id); return existing }
        let session = VideoSession(url: url); sessions.append(session); select(session.id); return session
    }
    func loadProject(_ url: URL, relink: URL? = nil) async throws -> VideoSession {
        var project = try await Task.detached(priority: .utility) { try ProjectFile.read(url) }.value
        if let relink { project.source = relink }
        guard FileManager.default.fileExists(atPath: project.source.path) else {
            throw AutomationError("relink_required", "Missing source footage: \(project.source.path). Supply relink_path; confirm it is the original video.")
        }
        try Task.checkCancellation()
        let session = VideoSession(url: project.source, project: project, projectURL: relink == nil ? url : nil)
        sessions.append(session); select(session.id)
        return session
    }
}
