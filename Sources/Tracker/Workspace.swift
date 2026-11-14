import TrackerAutomation
import AppKit
import AVFoundation
import Combine
import UniformTypeIdentifiers

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
    @Published private(set) var analysis = AnalysisState() { didSet { if oldValue != analysis { revision += 1 } } }
    @Published private(set) var revision = 0
    private(set) var lastTrackingID: UUID?
    var trackingJobs: [UUID: SessionTrackingJob] = [:]
    @Published private(set) var displayedFrame: DecodedFrame?
    @Published private(set) var frameTimes: [FrameStamp] = []
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
    private var trackingTask: Task<Void, Never>?
    private var seekGeneration = UUID()
    private var runID: UUID?
    private(set) var closed = false
    private var restored: ProjectFile?

    var tracks: [PointTrack] { analysis.tracks }
    var selectedTrackID: UUID? {
        get { analysis.selectedTrackID }
        set { if analysis.selectedTrackID != newValue { edit { $0.selectedTrackID = newValue } } }
    }
    var calibration: Calibration? {
        get { analysis.calibration }
        set { edit { $0.calibration = newValue }; calibrationPoints = [] }
    }
    var name: String { url.lastPathComponent }
    var selectedTrack: PointTrack? { tracks.first { $0.id == selectedTrackID } }
    var points: [Measurement] { selectedTrack?.points ?? [] }
    var unit: String { calibration?.unit ?? "px" }
    var isDirty: Bool { analysis != savedAnalysis || settings != savedSettings }
    var canMark: Bool { !loading && !isSeeking && !playing && displayedFrame != nil && error == nil && runID == nil }
    var canUndo: Bool { history.canUndo }
    var canRedo: Bool { history.canRedo }
    var isTracking: Bool { runID != nil }
    var seed: Measurement? {
        guard let stamp = displayedFrame?.stamp else { return nil }
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
            history.restore(project.analysis); analysis = project.analysis; savedAnalysis = project.analysis
            settings = project.settings; savedSettings = project.settings
        }
        timeObserver = player.addPeriodicTimeObserver(forInterval: CMTime(seconds: 0.1, preferredTimescale: 600), queue: .main) { [weak self] time in
            MainActor.assumeIsolated {
                guard let self, self.playing else { return }
                self.time = time.seconds.isFinite ? time.seconds : 0
                if self.player.rate == 0 { self.pause() }
            }
        }
        loadTask = Task { await load() }
    }

    func edit(_ change: (inout AnalysisState) -> Void) {
        cancelTracking()
        var next = analysis; change(&next)
        history.replace(next); analysis = history.state
    }
    func undo() { cancelTracking(); history.undo(); analysis = history.state; calibrationPoints = [] }
    func redo() { cancelTracking(); history.redo(); analysis = history.state; calibrationPoints = [] }

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
            frameTimes = stamps; duration = seconds
            let item = AVPlayerItem(asset: asset)
            statusObserver = item.observe(\.status, options: [.new]) { [weak self] item, _ in
                if item.status == .failed {
                    let message = item.error?.localizedDescription ?? "The video could not be decoded."
                    Task { @MainActor in self?.error = message }
                }
            }
            player.replaceCurrentItem(with: item)
            loading = false; seek(restored?.time ?? 0); restored = nil
        } catch {
            guard !closed, !Task.isCancelled else { return }
            self.error = "Could not open \(name). \(error.localizedDescription)"; loading = false
        }
    }

    func pause() {
        let refresh = playing
        player.pause(); playing = false
        if refresh { seek(player.currentTime().seconds) }
    }
    func togglePlayback() {
        if playing { pause(); return }
        guard !isSeeking, !loading, !isTracking else { return }
        if time >= (frameTimes.last?.seconds ?? duration) { seek(0); return }
        tool = .navigate; playing = true; player.play()
    }
    func seek(_ seconds: Double) {
        guard !frameTimes.isEmpty, !closed, seconds.isFinite else { return }
        player.pause(); playing = false
        let target = frameTimes[frameIndex(at: seconds)]
        if let review, abs(review.frame.stamp.seconds-target.seconds) > 0.000001 {
            self.review = nil; jobState = "Stopped"
        }
        seekTask?.cancel(); seekGeneration = UUID()
        let generation = seekGeneration
        isSeeking = true; frameError = nil; time = target.seconds
        player.seek(to: CMTime(value: target.value, timescale: target.timescale), toleranceBefore: .zero, toleranceAfter: .zero)
        seekTask = Task {
            do {
                let frame = try await media.image(at: target.seconds)
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
        var low = 0, high = frameTimes.count
        while low < high {
            let mid = (low+high)/2
            if frameTimes[mid].seconds <= time + 0.0000001 { low = mid+1 } else { high = mid }
        }
        return max(0, low-1)
    }
    func step(_ count: Int) {
        guard !frameTimes.isEmpty else { return }
        let index = min(frameTimes.count-1, max(0, frameIndex(at: time)+count))
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
        if let last = track.points.last, point.time > last.time + 0.000001 {
            track.points.append(point); return
        }
        var low = 0, high = track.points.count
        while low < high {
            let middle = (low+high)/2
            if track.points[middle].time < point.time - 0.000001 { low = middle+1 } else { high = middle }
        }
        if low < track.points.count, abs(track.points[low].time-point.time) < 0.000001 { track.points[low] = point }
        else { track.points.insert(point, at: low) }
    }
    func undoLastPoint() {
        edit { state in
            guard let index = state.tracks.firstIndex(where: { $0.id == state.selectedTrackID }), !state.tracks[index].points.isEmpty else { return }
            state.tracks[index].points.removeLast()
        }
    }
    func coordinates(_ point: CGPoint) -> CGPoint {
        let scale = calibration?.pixelsPerUnit ?? 1
        return CGPoint(x: point.x/scale, y: (size.height-point.y)/scale)
    }

    func startTracking(seed override: Measurement? = nil, after: Double? = nil, trackID explicitTrack: UUID? = nil) {
        guard let trackID = explicitTrack ?? selectedTrackID,
              let chosen = override ?? tracks.first(where: { $0.id == trackID })?.points.first(where: { abs($0.time - (displayedFrame?.stamp.seconds ?? -1)) < 0.000001 }), !loading, !isSeeking else {
            exportError = "Mark the displayed frame before starting autotracking."; return
        }
        pause(); cancelTracking()
        let token = UUID(); runID = token; lastTrackingID = token
        trackingJobs[token] = SessionTrackingJob(id: token, trackID: trackID, state: "Queued", processed: 0)
        trackingError = nil; jobState = "Queued"; review = nil
        processedFrames = 0; runSummary = ""
        let sourceURL = url, configuration = settings
        // One undo operation restores the analysis before the complete run.
        history.replace(analysis)
        let before = analysis
        trackingTask = Task.detached(priority: .userInitiated) { [weak self] in
            do {
                try await TrackingCapacity.shared.acquire(token)
                do {
                await self?.trackingStarted(token)
                try await TrackingEngine.run(url: sourceURL, seed: chosen, settings: configuration, after: after) { [weak self] event in
                    await self?.receive(event, token: token, trackID: trackID, before: before)
                }
                } catch { await TrackingCapacity.shared.release(token); throw error }
                await TrackingCapacity.shared.release(token)
            } catch {
                await self?.trackingFailed(error, token: token)
            }
        }
    }
    private func trackingStarted(_ token: UUID) { if runID == token { jobState = "Running" } }
    private func receive(_ event: TrackingEvent, token: UUID, trackID: UUID, before: AnalysisState) {
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
        for point in batch {
            // Manual corrections and explicitly reviewed measurements remain authoritative.
            if let old = next.tracks[index].points.first(where: { abs($0.time-point.time) < 0.000001 }), old.origin != .automatic { continue }
            Self.put(point, into: &next.tracks[index])
        }
        if next != analysis {
            // Coalesced delivery: one checkpoint per run, not per UI refresh.
            if analysis == before { history.replace(next) } else { history.updateCurrent(next) }
            analysis = next
        }
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
                                      score: review.evidence.score, frame: review.frame.stamp)
        edit { state in
            if let index = state.tracks.firstIndex(where: { $0.id == trackID }) { Self.put(measurement, into: &state.tracks[index]) }
        }
        startTracking(seed: measurement, trackID: trackID)
    }
    func skipCandidate() {
        guard let pending = review, let trackID = reviewTrackID, let previous = tracks.first(where: { $0.id == trackID })?.points.last(where: { $0.time < pending.frame.stamp.seconds }) else { return }
        startTracking(seed: previous, after: pending.frame.stamp.seconds, trackID: trackID)
    }

    func setSaving(_ value: Bool) { saving = value }
    func didSave(_ document: ProjectFile, at destination: URL) {
        projectURL = destination; savedAnalysis = document.analysis; savedSettings = document.settings
    }
    func snapshot() -> ProjectFile {
        ProjectFile(source: url, sourceSize: size, analysis: analysis, time: displayedFrame?.stamp.seconds ?? time, settings: settings)
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
        closed = true; cancelTracking(); loadTask?.cancel(); seekTask?.cancel(); seekGeneration = UUID()
        player.pause()
        if let timeObserver { player.removeTimeObserver(timeObserver); self.timeObserver = nil }
        statusObserver = nil; player.replaceCurrentItem(with: nil); displayedFrame = nil
    }
    func exportCSV() {
        let panel = NSSavePanel(); panel.allowedContentTypes = [.commaSeparatedText]
        panel.nameFieldStringValue = url.deletingPathExtension().lastPathComponent + "-points.csv"
        guard panel.runModal() == .OK, let destination = panel.url else { return }
        guard let trackID = selectedTrackID else { return }
        Task {
            do { try await writeCSV(trackID: trackID, to: destination, overwrite: true) }
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
