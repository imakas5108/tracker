import Foundation
import TrackerAutomation

@MainActor
final class SessionTrackingJob {
    var id: UUID
    var trackID: UUID
    var state: String
    var processed: Int
    var error: String?
    var revision = 0
    let settings: TrackingSettings
    let clip: VideoClip?
    init(id: UUID, trackID: UUID, state: String, processed: Int, settings: TrackingSettings = TrackingSettings(), clip: VideoClip? = nil) {
        self.id = id; self.trackID = trackID; self.state = state; self.processed = processed
        self.settings = settings
        self.clip = clip
    }
}

@MainActor
extension VideoSession {
    var reviewTrackID: UUID? { lastTrackingID.flatMap { trackingJobs[$0]?.trackID } }
    func recordTrackingState() {
        guard let id = lastTrackingID else { return }
        trackingJobs[id]?.state = jobState; trackingJobs[id]?.processed = processedFrames; trackingJobs[id]?.revision = revision
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
        edit { $0.tracks.append(track) }; selectedTrackID = track.id; tool = .mark
        return track
    }
    func recordPoint(_ point: CGPoint, trackID: UUID, time: Double) throws {
        try requireReady(); _ = try requireTrack(trackID); try validatePixel(point)
        guard canMark, let frame = displayedFrame, abs(frame.stamp.seconds-time) < 0.000001 else {
            throw AutomationError("frame_not_ready", "Pause and seek the native viewer to the requested source frame before marking.")
        }
        guard isSelected(frame.stamp) else { throw AutomationError("outside_clip", "Mark only selected clip steps; use Clip Settings to change the range or sampling.") }
        edit { state in
            let index = state.tracks.firstIndex(where: { $0.id == trackID })!
            Self.put(Measurement(time: frame.stamp.seconds, pixel: point, frame: frame.stamp), into: &state.tracks[index])
        }
    }
    func applyCalibration(_ value: Calibration) throws {
        try requireReady(); try validatePixel(value.first); try validatePixel(value.second)
        guard value.isValid, ["m","cm","mm"].contains(value.unit) else { throw AutomationError("invalid_calibration", "Supply distinct endpoints, a positive known length and m, cm or mm.") }
        guard coordinateSystem.isRepresentable(size:size,calibration:value) else { throw MotionError.nonfiniteCoordinates }
        calibration = value
    }
    func configure(_ value: TrackingSettings) throws {
        try requireReady()
        guard value.isValid else { throw AutomationError("invalid_settings", TrackingError.invalidSettings.localizedDescription) }
        settings = value
    }
    func beginTracking(trackID: UUID) throws -> UUID {
        try requireReady(); let track = try requireTrack(trackID)
        guard !isTracking, !isSeeking, !playing, let stamp = displayedFrame?.stamp, isSelected(stamp),
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
    func writeCSV(trackID: UUID, to destination: URL, overwrite: Bool, options: CSVExportOptions = CSVExportOptions()) async throws {
        try requireReady(); try validateOutput(destination)
        let options = try options.validated()
        let rows = try await motionRows(trackID:trackID)
        let units = unit, coordinate = coordinateSystem, configuration = motionSettings, dimensions = size, selection = clip
        try await Task.detached(priority: .utility) {
            let data = try MotionOutput.csv(rows,unit:units,coordinates:coordinate,size:dimensions,settings:configuration,options:options,clip:selection)
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
