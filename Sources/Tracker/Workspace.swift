import TrackerAutomation
import AppKit
import AVFoundation
import Combine
import UniformTypeIdentifiers
import SwiftUI

enum WorkspaceTool: String, CaseIterable {
    case navigate = "Navigate", mark = "Mark points", calibrate = "Calibrate"
    var symbol: String {
        switch self { case .navigate: "cursorarrow"; case .mark: "plus.circle"; case .calibrate: "ruler" }
    }
}

@MainActor
final class VideoSession: ObservableObject, Identifiable {
    let id = UUID()
    let url: URL
    let player = AVPlayer()
    private let media: MediaSource
    @Published var size = CGSize(width: 16, height: 9)
    @Published var duration = 0.0
    @Published var time = 0.0
    @Published var playing = false
    @Published var loading = true
    @Published var isSeeking = false
    @Published var error: String?
    @Published var exportError: String?
    @Published private(set) var frameError: String?
    @Published private(set) var trackingError: String?
    @Published var tool: WorkspaceTool = .navigate
    @Published var showAxes = false
    @Published var calibrationPoints: [CGPoint] = []
    @Published var showCalibration = false
    @Published private(set) var analysis = AnalysisState() {
        didSet {
            if oldValue != analysis {
                revision += 1
                selectedPointCache = nil
                if oldValue.clip != analysis.clip { clipChanged() }
                if isTracking || review != nil { recordTrackingState() }
                refreshMotion(preservingRows:preserveMotionOnNextChange)
            }
            preserveMotionOnNextChange = false
        }
    }
    @Published private(set) var revision = 0
    private(set) var lastTrackingID: UUID?
    var trackingJobs: [UUID: SessionTrackingJob] = [:]
    @Published private(set) var displayedFrame: DecodedFrame?
    @Published private(set) var frameTimes: [FrameStamp] = []
    private(set) var analysisFrameTimes: [FrameStamp] = []
    private(set) var selectedFrameSeconds: Set<Double> = []
    private var selectedPointCache: (revision: Int, trackID: UUID?, points: [Measurement])?
    @Published var projectURL: URL?
    @Published private(set) var savedAnalysis = AnalysisState()
    @Published var settings = TrackingSettings() { didSet { if oldValue != settings { revision += 1; cancelTracking() } } }
    @Published private(set) var savedSettings = TrackingSettings()
    @Published private(set) var saving = false
    @Published private(set) var jobState = "Idle" { didSet { recordTrackingState() } }
    @Published private(set) var processedFrames = 0 { didSet { recordTrackingState() } }
    @Published private(set) var runSummary = ""
    @Published private(set) var review: TrackingReview?
    private var history = AnalysisHistory()
    private var timeObserver: Any?
    private var statusObserver: NSKeyValueObservation?
    private var loadTask: Task<Void, Never>?
    private var seekTask: Task<Void, Never>?
    private var sampledPlaybackTask: Task<Void, Never>?
    private var playbackGeneration = UUID()
    private var trackingTask: Task<Void, Never>?
    private var seekGeneration = UUID()
    private var runID: UUID?
    private(set) var closed = false
    private var restored: ProjectFile?
    let motionCalculator = MotionCalculator()
    let motionData = MotionPresentation()
    private var motionTask: Task<Void, Never>?
    private var motionGeneration = UUID()
    private var trackingCheckpointNeeded = true
    private var preserveMotionOnNextChange = false
    var coordinateSystem: CoordinateSystem { analysis.coordinates ?? CoordinateSystem() }
    var motionSettings: MotionSettings { analysis.motion ?? MotionSettings() }

    var tracks: [PointTrack] { analysis.tracks }
    @Published var selectedTrackID: UUID? {
        didSet {
            if let id = selectedTrackID, !tracks.contains(where: { $0.id == id }) { selectedTrackID = nil }
            if oldValue != selectedTrackID { refreshMotion() }
        }
    }
    private func reconcileSelection() {
        if !tracks.contains(where: { $0.id == selectedTrackID }) { selectedTrackID = tracks.first?.id }
    }
    var calibration: Calibration? {
        get { analysis.calibration }
        set {
            guard coordinateSystem.isRepresentable(size:size,calibration:newValue) else {
                exportError = MotionError.nonfiniteCoordinates.localizedDescription; return
            }
            editDerived { $0.calibration = newValue }; calibrationPoints = []
        }
    }
    var name: String { url.lastPathComponent }
    var selectedTrack: PointTrack? { tracks.first { $0.id == selectedTrackID } }
    var points: [Measurement] {
        if let cached = selectedPointCache, cached.revision == revision, cached.trackID == selectedTrackID { return cached.points }
        let result = selectedObservations(selectedTrack?.points ?? [])
        selectedPointCache = (revision,selectedTrackID,result)
        return result
    }
    var unit: String { calibration?.unit ?? "px" }
    var isDirty: Bool { analysis != savedAnalysis || settings != savedSettings }
    var canMark: Bool { !loading && !isSeeking && !playing && displayedFrame != nil && error == nil && runID == nil }
    var canUndo: Bool { history.canUndo }
    var canRedo: Bool { history.canRedo }
    var isTracking: Bool { runID != nil }
    var seed: Measurement? {
        guard let stamp = displayedFrame?.stamp, isSelected(stamp) else { return nil }
        return points.first { abs($0.time - stamp.seconds) < 0.000001 }
    }
    var status: String {
        if loading { return "Opening video and indexing source timestamps…" }
        if error != nil { return "Video unavailable" }
        if let frameError { return frameError }
        if isSeeking { return "Loading source frame…" }
        if review != nil { return "Tracking paused: review the uncertain match." }
        if isTracking { return "\(jobState) · \(processedFrames) frames inspected" }
        switch tool {
        case .navigate: return "Choose Mark points to record a position."
        case .mark: return "Mark the displayed frame, then step forward; Autotracker starts from a marked frame."
        case .calibrate: return "Select two endpoints of a known distance."
        }
    }

    init(url: URL, project: ProjectFile? = nil, projectURL: URL? = nil) {
        self.url = url; self.media = MediaSource(url: url); self.restored = project; self.projectURL = projectURL
        if let project {
            history.restore(project.analysis.scientificState); analysis = project.analysis.scientificState
            savedAnalysis = analysis; selectedTrackID = project.analysis.selectedTrackID
            settings = project.settings; savedSettings = project.settings
        }
        timeObserver = player.addPeriodicTimeObserver(forInterval: CMTime(seconds: 0.1, preferredTimescale: 600), queue: .main) { [weak self] time in
            MainActor.assumeIsolated {
                guard let self, self.playing, !self.usesSampledPlayback, let clip = self.clip else { return }
                if self.player.rate == 0 || time.seconds >= clip.lastSampleStamp.seconds {
                    self.stopPlayback(); self.seek(clip.lastSampleStamp.seconds)
                } else { self.time = max(clip.startStamp.seconds,time.seconds.isFinite ? time.seconds : clip.startStamp.seconds) }
            }
        }
        loadTask = Task { await load() }
    }

    func edit(_ change: (inout AnalysisState) -> Void) {
        cancelTracking()
        var next = analysis; change(&next)
        history.replace(next.scientificState); analysis = history.state; reconcileSelection()
    }
    func undo() { cancelTracking(); history.undo(); analysis = history.state; reconcileSelection(); calibrationPoints = [] }
    func redo() { cancelTracking(); history.redo(); analysis = history.state; reconcileSelection(); calibrationPoints = [] }

    func editDerived(_ change: (inout AnalysisState) -> Void) {
        var next = analysis; change(&next)
        history.replace(next.scientificState); analysis = history.state
        trackingCheckpointNeeded = true
    }
    func setCoordinates(_ value: CoordinateSystem) throws {
        try requireReady()
        guard value.isValid else { throw MotionError.invalidSettings }
        guard value.isRepresentable(size:size,calibration:calibration) else { throw MotionError.nonfiniteCoordinates }
        editDerived { $0.coordinates = value }
    }
    func setMotionSettings(_ value: MotionSettings) throws {
        try requireReady()
        guard value.isValid else { throw MotionError.invalidSettings }
        editDerived { $0.motion = value }
    }
    func motionRows(trackID: UUID) async throws -> [MotionRow] {
        try requireReady()
        let points = selectedObservations(try requireTrack(trackID).points)
        let key = MotionKey(revision:revision,trackID:trackID)
        let rows = try await motionCalculator.rows(key:key,points:points,sourceTimes:analysisFrameTimes,size:size,
            calibration:calibration,coordinates:coordinateSystem,settings:motionSettings)
        guard !closed, revision == key.revision else {
            throw AutomationError("stale_revision", "Measurements changed during calculation; inspect and retry.")
        }
        return rows
    }
    private func refreshMotion(preservingRows: Bool = false) {
        motionTask?.cancel(); motionGeneration = UUID()
        let generation = motionGeneration
        if !preservingRows { motionData.clear() }
        guard !closed, !loading, let trackID = selectedTrackID else { return }
        let calculator = motionCalculator, points = points, sourceTimes = analysisFrameTimes, size = size
        let calibration = calibration, coordinates = coordinateSystem, configuration = motionSettings
        let key = MotionKey(revision:revision,trackID:trackID)
        motionData.updating = true
        motionTask = Task { [weak self] in
            do {
                try await Task.sleep(for:.milliseconds(80))
                let rows = try await calculator.rows(key:key,points:points,sourceTimes:sourceTimes,size:size,
                    calibration:calibration,coordinates:coordinates,settings:configuration)
                try Task.checkCancellation()
                guard let self, !self.closed, self.motionGeneration == generation else { return }
                self.motionData.set(rows,revision:key.revision)
            } catch {
                guard let self, self.motionGeneration == generation else { return }
                self.motionData.updating = false
                if !(error is CancellationError) { self.motionData.error = error.localizedDescription }
            }
        }
    }

    private func load() async {
        do {
            let asset = AVURLAsset(url: url)
            guard try await asset.load(.isPlayable), let track = try await asset.loadTracks(withMediaType: .video).first else { throw MediaError.noFrames }
            let sourceSize = try await track.load(.naturalSize)
            let transform = try await track.load(.preferredTransform)
            let dimensions = sourceSize.applying(transform)
            let seconds = try await asset.load(.duration).seconds
            let sourceURL = url
            let indexTask = Task.detached(priority: .utility) { try await MediaSource.timestamps(url: sourceURL) }
            let stamps = try await withTaskCancellationHandler { try await indexTask.value } onCancel: { indexTask.cancel() }
            try Task.checkCancellation()
            guard !closed, seconds.isFinite, seconds > 0 else { throw MediaError.noFrames }
            size = CGSize(width: abs(dimensions.width), height: abs(dimensions.height))
            if let restored, restored.sourceSize != size { throw ProjectError.sourceMismatch }
            try analysis.clip?.validate(source:stamps)
            frameTimes = stamps; duration = seconds; rebuildClipTimeline(); refreshMotion()
            let item = AVPlayerItem(asset: asset)
            statusObserver = item.observe(\.status, options: [.new]) { [weak self] item, _ in
                if item.status == .failed {
                    let message = item.error?.localizedDescription ?? "The video could not be decoded."
                    Task { @MainActor in self?.error = message }
                }
            }
            player.replaceCurrentItem(with: item)
            updatePlaybackBoundary()
            loading = false; refreshMotion(); seek(restored?.time ?? 0); restored = nil
        } catch {
            guard !closed, !Task.isCancelled else { return }
            self.error = "Could not open \(name). \(error.localizedDescription)"; loading = false; refreshMotion()
        }
    }

    func pause() {
        let refresh = playing
        let sourceTime = usesSampledPlayback ? time : player.currentTime().seconds
        stopPlayback()
        if refresh { seek(sourceTime) }
    }
    private func stopPlayback() {
        playbackGeneration = UUID(); sampledPlaybackTask?.cancel(); sampledPlaybackTask = nil
        player.pause(); playing = false
    }
    private func rebuildClipTimeline() {
        selectedPointCache = nil
        analysisFrameTimes = (try? clip?.timestamps(in:frameTimes)) ?? []
        selectedFrameSeconds = Set(analysisFrameTimes.map(\.seconds))
    }
    private func updatePlaybackBoundary() {
        guard let clip, let item = player.currentItem else { return }
        let next = clip.lastSampleFrame+1
        item.forwardPlaybackEndTime = next < frameTimes.count ? frameTimes[next].cmTime : CMTime(seconds:duration,preferredTimescale:600000)
    }
    private func clipChanged() {
        rebuildClipTimeline()
        guard !loading, !closed, !frameTimes.isEmpty else { return }
        stopPlayback(); cancelTracking(); cancelSeek(); updatePlaybackBoundary(); seek(time)
    }
    func togglePlayback() {
        if playing { pause(); return }
        guard !isSeeking, !loading, !isTracking, let clip, clip.sampleCount > 1 else { return }
        if time >= clip.lastSampleStamp.seconds { seek(clip.startStamp.seconds); return }
        tool = .navigate; playing = true
        if usesSampledPlayback { playSampledFrames() } else { player.play() }
    }
    private func playSampledFrames() {
        let generation = UUID(); playbackGeneration = generation
        let frames = analysisFrameTimes, startTime = time, media = media
        let clock = ContinuousClock(), began = ContinuousClock.now
        sampledPlaybackTask = Task { [weak self] in
            do {
                while true {
                    try Task.checkCancellation()
                    let elapsed = began.duration(to:clock.now)
                    let seconds = Double(elapsed.components.seconds)+Double(elapsed.components.attoseconds)/1e18
                    let index = MediaSource.frameIndex(at:startTime+seconds,in:frames)
                    let frame = try await media.image(at:frames[index])
                    guard abs(frame.stamp.seconds-frames[index].seconds) < 0.000001 else { throw MediaError.decode }
                    guard let self, !self.closed, self.playbackGeneration == generation else { return }
                    self.displayedFrame = frame; self.time = frame.stamp.seconds
                    if index == frames.count-1 { self.playing = false; self.sampledPlaybackTask = nil; return }
                    let next = began.advanced(by:.seconds(frames[index+1].seconds-startTime))
                    try await clock.sleep(until:next)
                }
            } catch {
                guard let self, self.playbackGeneration == generation else { return }
                self.playing = false; self.sampledPlaybackTask = nil
                if !(error is CancellationError) { self.frameError = error.localizedDescription }
            }
        }
    }
    func seek(_ seconds: Double) {
        guard !frameTimes.isEmpty, !closed, seconds.isFinite else { return }
        stopPlayback()
        guard let clip else { return }
        let target = frameTimes[clip.frameAtOrBefore(frameIndex(at:seconds))]
        if let review, abs(review.frame.stamp.seconds-target.seconds) > 0.000001 {
            self.review = nil; jobState = "Stopped"
        }
        seekTask?.cancel(); seekGeneration = UUID()
        let generation = seekGeneration
        isSeeking = true; frameError = nil; time = target.seconds
        player.seek(to: CMTime(value: target.value, timescale: target.timescale), toleranceBefore: .zero, toleranceAfter: .zero)
        seekTask = Task {
            do {
                let frame = try await media.image(at: target)
                guard generation == seekGeneration, !closed, !Task.isCancelled else { return }
                guard abs(frame.stamp.seconds - target.seconds) < 0.000001 else { throw MediaError.decode }
                displayedFrame = frame; time = frame.stamp.seconds; isSeeking = false
            } catch {
                guard generation == seekGeneration, !closed, !Task.isCancelled else { return }
                isSeeking = false; frameError = "Frame read failed: \(error.localizedDescription)"
                displayedFrame = nil
            }
        }
    }
    var seekToken: UUID { seekGeneration }
    func cancelSeek() { seekTask?.cancel(); seekGeneration = UUID(); isSeeking = false; displayedFrame = nil }
    private func frameIndex(at time: Double) -> Int {
        MediaSource.frameIndex(at: time, in: frameTimes)
    }

    func step(_ count: Int) {
        guard !frameTimes.isEmpty else { return }
        guard let clip else { return }
        let index = clip.advanced(from:frameIndex(at:time),steps:count)
        seek(frameTimes[index].seconds)
    }
    func addTrack() {
        _ = createTrack(name: "Point mass \(tracks.count+1)")
    }
    func selectTool(_ value: WorkspaceTool) {
        pause(); tool = value
        if value == .mark && selectedTrackID == nil { addTrack() }
        if value == .calibrate { calibrationPoints = [] }
    }
    func mark(_ point: CGPoint) {
        guard canMark, point.x.isFinite, point.y.isFinite, point.x >= 0, point.y >= 0,
              point.x < size.width, point.y < size.height, let frame = displayedFrame else { return }
        if tool == .calibrate {
            if calibrationPoints.count >= 2 { calibrationPoints = [] }
            calibrationPoints.append(point)
            if calibrationPoints.count == 2 { showCalibration = true }
        } else if tool == .mark, let trackID = selectedTrackID {
            try? recordPoint(point, trackID: trackID, time: frame.stamp.seconds)
        }
    }
    static func put(_ point: Measurement, into track: inout PointTrack) {
        track.put(point)
    }

    func undoLastPoint() {
        let trackID = selectedTrackID
        guard let pointID = points.last?.id else { return }
        edit { state in
            guard let index = state.tracks.firstIndex(where: { $0.id == trackID }) else { return }
            state.tracks[index].points.removeAll { $0.id == pointID }
        }
    }
    func coordinates(_ point: CGPoint) -> CGPoint {
        coordinateSystem.convert(point,size:size,calibration:calibration)
    }

    func startTracking(seed override: Measurement? = nil, after: Double? = nil, trackID explicitTrack: UUID? = nil) {
        guard let trackID = explicitTrack ?? selectedTrackID,
              let chosen = override ?? tracks.first(where: { $0.id == trackID })?.points.first(where: { abs($0.time - (displayedFrame?.stamp.seconds ?? -1)) < 0.000001 }), !loading, !isSeeking, !playing else {
            exportError = "Mark the displayed frame before starting autotracking."; return
        }
        guard let clip, !selectedObservations([chosen]).isEmpty else {
            exportError = "The tracking seed must be on a selected clip step."; return
        }
        pause(); cancelTracking()
        let token = UUID(); runID = token; lastTrackingID = token
        trackingJobs[token] = SessionTrackingJob(id: token, trackID: trackID, state: "Queued", processed: 0, settings: settings, clip:clip)
        trackingError = nil; jobState = "Queued"; review = nil
        processedFrames = 0; runSummary = ""
        let sourceURL = url, configuration = settings, sourceTimes = frameTimes
        trackingCheckpointNeeded = true
        trackingTask = Task.detached(priority: .userInitiated) { [weak self] in
            do {
                try await TrackingCapacity.shared.acquire(token)
                do {
                await self?.trackingStarted(token)
                try await TrackingEngine.run(url: sourceURL, seed: chosen, settings: configuration, after: after, clip:clip, sourceTimes:sourceTimes) { [weak self] event in
                    await self?.receive(event, token: token, trackID: trackID)
                }
                } catch { await TrackingCapacity.shared.release(token); throw error }
                await TrackingCapacity.shared.release(token)
            } catch {
                await self?.trackingFailed(error, token: token)
            }
        }
    }
    private func trackingStarted(_ token: UUID) { if runID == token { jobState = "Running" } }
    private func receive(_ event: TrackingEvent, token: UUID, trackID: UUID) {
        guard runID == token, !closed, let index = analysis.tracks.firstIndex(where: { $0.id == trackID }) else { return }
        let batch: [Measurement]
        switch event {
        case .update(let rows, _, let count): batch = rows; processedFrames = count
        case .review(let rows, let pending, let count):
            batch = rows; processedFrames = count; review = pending; jobState = "Awaiting review"
            runID = nil; trackingTask = nil; seek(pending.frame.stamp.seconds)
        case .complete(let rows, let count, let seconds):
            batch = rows; processedFrames = count; runID = nil; trackingTask = nil; jobState = "Completed"
            runSummary = String(format: "%d frames · %.2f s · %.1f frames/s", count, seconds, Double(count)/max(seconds, 0.000001))
        }
        var next = analysis
        let appendOnly = batch.first.map { $0.time > (analysis.tracks[index].points.last?.time ?? -.infinity)+0.000001 } ?? false
        next.tracks[index].mergeAutomatic(batch)
        if next != analysis {
            // Explicit derived-data edits split the run into independently undoable checkpoints.
            if trackingCheckpointNeeded { history.replace(next); trackingCheckpointNeeded = false } else { history.updateCurrent(next) }
            preserveMotionOnNextChange = appendOnly && trackID == selectedTrackID
            analysis = next
        }
        recordTrackingState()
    }
    private func trackingFailed(_ error: Error, token: UUID) {
        guard runID == token else { return }
        runID = nil; trackingTask = nil
        if !(error is CancellationError) {
            trackingError = error.localizedDescription
            trackingJobs[token]?.error = error.localizedDescription
        }
        jobState = error is CancellationError ? "Cancelled" : "Failed"
    }
    func cancelTracking() {
        if runID != nil || review != nil { jobState = "Stopped" }
        runID = nil; trackingTask?.cancel(); trackingTask = nil; review = nil
    }
    func acceptCandidate() {
        guard let review, let pixel = review.evidence.position, let trackID = reviewTrackID else { return }
        let measurement = Measurement(time: review.frame.stamp.seconds, pixel: pixel, origin: .acceptedCandidate,
                                      score: review.evidence.score, frame: review.frame.stamp, matchingMethod: settings.matchingMethod)
        edit { state in
            if let index = state.tracks.firstIndex(where: { $0.id == trackID }) { Self.put(measurement, into: &state.tracks[index]) }
        }
        startTracking(seed: measurement, trackID: trackID)
    }
    func skipCandidate() {
        guard let pending = review, let trackID = reviewTrackID,
              let track = tracks.first(where: { $0.id == trackID }),
              let previous = selectedObservations(track.points).last(where: { $0.time < pending.frame.stamp.seconds }) else { return }
        startTracking(seed: previous, after: pending.frame.stamp.seconds, trackID: trackID)
    }

    func setSaving(_ value: Bool) { saving = value }
    func didSave(_ document: ProjectFile, at destination: URL) {
        projectURL = destination; savedAnalysis = document.analysis.scientificState; savedSettings = document.settings
    }
    func snapshot() -> ProjectFile {
        var persisted = analysis; persisted.selectedTrackID = selectedTrackID
        let needsVersion3 = settings.matchingMethod == .normalizedCorrelation || persisted.tracks.contains { $0.points.contains { $0.matchingMethod == .normalizedCorrelation } }
        return ProjectFile(version: persisted.clip != nil ? 4 : needsVersion3 ? 3 : 2, source: url, sourceSize: size, analysis: persisted, time: displayedFrame?.stamp.seconds ?? time, settings: settings, algorithm: settings.matchingMethod.algorithm)
    }
    func saveProject() async -> Bool {
        guard !loading, !saving else { return false }
        var destination = projectURL
        if destination == nil {
            let panel = NSSavePanel(); panel.allowedContentTypes = [.json]
            panel.nameFieldStringValue = url.deletingPathExtension().lastPathComponent + ".tracker.json"
            guard panel.runModal() == .OK else { return false }; destination = panel.url
        }
        guard let destination else { return false }
        do { try await writeProject(to: destination, overwrite: true); return true }
        catch { exportError = "Save failed: \(error.localizedDescription)"; return false }
    }

    func confirmClose() async -> Bool {
        guard !saving else { return false }
        guard isDirty else { return true }
        cancelTracking()
        let alert = NSAlert()
        alert.messageText = "Save changes to \(name)?"
        alert.informativeText = "Closing without saving discards the analysis. CSV export does not save project settings."
        alert.addButton(withTitle: "Save"); alert.addButton(withTitle: "Cancel"); alert.addButton(withTitle: "Discard")
        switch alert.runModal() {
        case .alertFirstButtonReturn: return await saveProject()
        case .alertThirdButtonReturn: return true
        default: return false
        }
    }
    func close() {
        stopPlayback()
        closed = true; motionTask?.cancel(); cancelTracking(); loadTask?.cancel(); seekTask?.cancel(); seekGeneration = UUID()
        player.pause()
        if let timeObserver { player.removeTimeObserver(timeObserver); self.timeObserver = nil }
        statusObserver = nil; player.replaceCurrentItem(with: nil); displayedFrame = nil
    }
    func exportCSV() {
        guard let trackID = selectedTrackID else { return }
        let preferences = CSVExportPreferences()
        let panel = NSSavePanel(); panel.allowedContentTypes = [.commaSeparatedText]
        let accessory = NSHostingView(rootView:CSVExportControls(preferences:preferences))
        accessory.frame = CGRect(x:0,y:0,width:450,height:280)
        panel.accessoryView = accessory
        panel.nameFieldStringValue = url.deletingPathExtension().lastPathComponent + "-points.csv"
        guard panel.runModal() == .OK, let destination = panel.url else { return }
        let options = preferences.options
        Task {
            do { try await writeCSV(trackID: trackID, to: destination, overwrite: true, options:options) }
            catch { exportError = "Export failed: \(error.localizedDescription)" }
        }
    }

}

@MainActor
final class Workspace: ObservableObject {
    @Published var sessions: [VideoSession] = []
    lazy var automation = AutomationController(workspace: self)
    @Published var selectedID: UUID?
    @Published var message: String?
    var selected: VideoSession? { sessions.first { $0.id == selectedID } }
    func openVideos() {
        let panel = NSOpenPanel(); panel.allowedContentTypes = [.movie, .video]; panel.allowsMultipleSelection = true
        if panel.runModal() == .OK { importVideos(panel.urls) }
    }
    func importVideos(_ urls: [URL]) {
        for url in urls {
            if url.pathExtension.lowercased() == "json" { openProject(url); continue }
            _ = importVideo(url)
        }
    }
    func chooseProject() {
        let panel = NSOpenPanel(); panel.allowedContentTypes = [.json]
        if panel.runModal() == .OK, let url = panel.url { openProject(url) }
    }
    func openProject(_ url: URL) {
        Task {
            do {
                let project = try await Task.detached(priority: .utility) { try ProjectFile.read(url) }.value
                var replacementURL: URL?
                if !FileManager.default.fileExists(atPath: project.source.path) {
                    let panel = NSOpenPanel(); panel.allowedContentTypes = [.movie, .video]
                    panel.message = "Locate the source video for \(url.lastPathComponent)."
                    guard panel.runModal() == .OK, let replacement = panel.url else { return }
                    replacementURL = replacement
                }
                _ = try await loadProject(url, relink: replacementURL)
                if replacementURL != nil { message = "Video relinked. Save to keep the new source reference. Confirm it is the original footage." }
            } catch { message = "Could not open project: \(error.localizedDescription)" }
        }
    }
    func select(_ id: UUID) { selected?.pause(); selectedID = id }
    func close(_ id: UUID) {
        Task {
            guard let session = sessions.first(where: { $0.id == id }), await session.confirmClose() else { return }
            session.close(); sessions.removeAll { $0.id == id }
            if selectedID == id { selectedID = sessions.last?.id }
        }
    }
    func confirmQuit() async -> Bool {
        for session in sessions { if !(await session.confirmClose()) { return false } }
        sessions.forEach { $0.close() }; sessions.removeAll(); selectedID = nil
        return true
    }
}
