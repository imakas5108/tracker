import AppKit
import AVFoundation
import Combine
import UniformTypeIdentifiers

enum WorkspaceTool: String, CaseIterable {
    case navigate = "Navigate", mark = "Mark points", calibrate = "Calibrate"
    var symbol: String {
        switch self {
        case .navigate: "cursorarrow"
        case .mark: "plus.circle"
        case .calibrate: "ruler"
        }
    }
}

@MainActor
final class VideoSession: ObservableObject, Identifiable {
    let id = UUID()
    let url: URL
    let player = AVPlayer()
    @Published var size = CGSize(width: 16, height: 9)
    @Published var duration = 0.0
    @Published var time = 0.0
    @Published var playing = false
    @Published var loading = true
    @Published var error: String?
    @Published var exportError: String?
    @Published var tracks: [PointTrack] = []
    @Published var selectedTrackID: UUID?
    @Published var tool: WorkspaceTool = .navigate
    @Published var showAxes = false
    @Published var calibration: Calibration?
    @Published var calibrationPoints: [CGPoint] = []
    @Published var showCalibration = false
    private var timeObserver: Any?
    private var statusObserver: NSKeyValueObservation?
    private var loadTask: Task<Void, Never>?

    var name: String { url.lastPathComponent }
    var selectedTrack: PointTrack? { tracks.first { $0.id == selectedTrackID } }
    var points: [Measurement] { selectedTrack?.points ?? [] }
    var unit: String { calibration?.unit ?? "px" }
    var status: String {
        if loading { return "Opening video…" }
        if error != nil { return "Video unavailable" }
        switch tool {
        case .navigate: return "Choose Mark points to record a position."
        case .mark: return "Click the video to mark a point; step forward to continue."
        case .calibrate: return "Select two endpoints of a known distance."
        }
    }

    init(url: URL) {
        self.url = url
        timeObserver = player.addPeriodicTimeObserver(forInterval: CMTime(seconds: 0.1, preferredTimescale: 600), queue: .main) { [weak self] time in
            MainActor.assumeIsolated {
                guard let self else { return }
                self.time = time.seconds.isFinite ? time.seconds : 0
                self.playing = self.player.rate != 0
            }
        }
        loadTask = Task { await load() }
    }

    func load() async {
        do {
            let asset = AVURLAsset(url: url)
            guard try await asset.load(.isPlayable),
                  let track = try await asset.loadTracks(withMediaType: .video).first else {
                throw CocoaError(.fileReadCorruptFile)
            }
            let sourceSize = try await track.load(.naturalSize)
            let transform = try await track.load(.preferredTransform)
            let transformed = sourceSize.applying(transform)
            let seconds = try await asset.load(.duration).seconds
            try Task.checkCancellation()
            guard seconds.isFinite, seconds > 0 else { throw CocoaError(.fileReadCorruptFile) }
            size = CGSize(width: abs(transformed.width), height: abs(transformed.height))
            duration = seconds
            let item = AVPlayerItem(asset: asset)
            statusObserver = item.observe(\.status, options: [.new]) { [weak self] item, _ in
                if item.status == .failed {
                    let message = item.error?.localizedDescription ?? "The video could not be decoded."
                    Task { @MainActor in self?.error = message }
                }
            }
            player.replaceCurrentItem(with: item)
            loading = false
        } catch {
            guard !Task.isCancelled else { return }
            self.error = "Could not open \(name). \(error.localizedDescription)"
            loading = false
        }
    }

    func pause() { player.pause(); playing = false }
    func togglePlayback() {
        if playing { pause() } else {
            if time >= duration - 0.01 { seek(0) }
            player.play(); playing = true
        }
    }
    func seek(_ seconds: Double) {
        let target = min(max(seconds, 0), duration)
        time = target
        player.seek(to: CMTime(seconds: target, preferredTimescale: 60000), toleranceBefore: .zero, toleranceAfter: .zero)
    }
    func step(_ count: Int) { pause(); player.currentItem?.step(byCount: count) }
    func addTrack() {
        let track = PointTrack(name: "Point mass \(tracks.count + 1)")
        tracks.append(track); selectedTrackID = track.id; tool = .mark
    }
    func selectTool(_ value: WorkspaceTool) {
        pause(); tool = value
        if value == .mark && selectedTrackID == nil { addTrack() }
        if value == .calibrate { calibrationPoints = [] }
    }
    func mark(_ point: CGPoint) {
        pause()
        if tool == .calibrate {
            if calibrationPoints.count >= 2 { calibrationPoints = [] }
            calibrationPoints.append(point)
            if calibrationPoints.count == 2 { showCalibration = true }
        } else if tool == .mark, let index = tracks.firstIndex(where: { $0.id == selectedTrackID }) {
            let timestamp = player.currentTime().seconds
            guard timestamp.isFinite else { return }
            // One position per source timestamp. No nominal-FPS conversion for VFR media.
            tracks[index].points.removeAll { abs($0.time - timestamp) < 0.00001 }
            tracks[index].points.append(Measurement(time: timestamp, pixel: point))
            tracks[index].points.sort { $0.time < $1.time }
        }
    }
    func undoLastPoint() {
        guard let index = tracks.firstIndex(where: { $0.id == selectedTrackID }), !tracks[index].points.isEmpty else { return }
        tracks[index].points.removeLast()
    }
    func coordinates(_ point: CGPoint) -> CGPoint {
        let scale = calibration?.pixelsPerUnit ?? 1
        return CGPoint(x: point.x / scale, y: (size.height - point.y) / scale)
    }
    func close() {
        loadTask?.cancel(); loadTask = nil
        pause()
        if let timeObserver { player.removeTimeObserver(timeObserver); self.timeObserver = nil }
        statusObserver = nil
        player.replaceCurrentItem(with: nil)
    }
    func exportCSV() {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.commaSeparatedText]
        panel.nameFieldStringValue = url.deletingPathExtension().lastPathComponent + "-points.csv"
        guard panel.runModal() == .OK, let destination = panel.url else { return }
        let rows = points.map { point in
            let position = coordinates(point.pixel)
            return "\(point.time),\(position.x),\(position.y),manual"
        }
        let contents = "time_s,x_\(unit),y_\(unit),source\n" + rows.joined(separator: "\n") + "\n"
        Task {
            do {
                try await Task.detached(priority: .utility) {
                    try contents.write(to: destination, atomically: true, encoding: .utf8)
                }.value
            } catch { exportError = "Export failed. \(error.localizedDescription)" }
        }
    }
}

@MainActor
final class Workspace: ObservableObject {
    @Published var sessions: [VideoSession] = []
    @Published var selectedID: UUID?
    var selected: VideoSession? { sessions.first { $0.id == selectedID } }

    func openVideos() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.movie, .video]
        panel.allowsMultipleSelection = true
        guard panel.runModal() == .OK else { return }
        importVideos(panel.urls)
    }
    func importVideos(_ urls: [URL]) {
        for url in urls {
            if let existing = sessions.first(where: { $0.url == url }) { select(existing.id); continue }
            let session = VideoSession(url: url)
            sessions.append(session); select(session.id)
        }
    }
    func select(_ id: UUID) { selected?.pause(); selectedID = id }
    func close(_ id: UUID) {
        guard let index = sessions.firstIndex(where: { $0.id == id }) else { return }
        sessions[index].close(); sessions.remove(at: index)
        if selectedID == id { selectedID = sessions.last?.id }
    }
}
