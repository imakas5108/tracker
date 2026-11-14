import Foundation
import CoreGraphics
import CoreMedia

enum MeasurementOrigin: String, Codable { case manual, automatic, acceptedCandidate }

struct FrameStamp: Codable, Equatable, Hashable, Comparable {
    var value: Int64
    var timescale: Int32
    var cmTime: CMTime { CMTime(value: value, timescale: timescale) }
    var seconds: Double { Double(value) / Double(timescale) }
    static func < (lhs: Self, rhs: Self) -> Bool { lhs.seconds < rhs.seconds }
}

struct Measurement: Identifiable, Codable, Equatable {
    var id = UUID()
    let time: Double
    let pixel: CGPoint
    var origin: MeasurementOrigin = .manual
    var score: Double? = nil
    var frame: FrameStamp? = nil
}

struct PointTrack: Identifiable, Codable, Equatable {
    var id = UUID()
    var name: String
    var points: [Measurement] = []
}

struct Calibration: Codable, Equatable {
    var first: CGPoint
    var second: CGPoint
    var length: Double
    var unit: String

    var pixelsPerUnit: Double { hypot(second.x - first.x, second.y - first.y) / length }
    var isValid: Bool { length.isFinite && length > 0 && pixelsPerUnit.isFinite && pixelsPerUnit > 0 }
}

struct AnalysisState: Codable, Equatable {
    var tracks: [PointTrack] = []
    var selectedTrackID: UUID?
    var calibration: Calibration?
    var coordinates: CoordinateSystem?
    var motion: MotionSettings?
    var scientificState: Self {
        var state = self; state.selectedTrackID = nil; return state
    }
}

struct AnalysisHistory {
    private(set) var state = AnalysisState()
    private var past: [AnalysisState] = []
    private var future: [AnalysisState] = []
    var canUndo: Bool { !past.isEmpty }
    var canRedo: Bool { !future.isEmpty }
    mutating func replace(_ next: AnalysisState) {
        guard next != state else { return }
        past.append(state); future.removeAll(); state = next
    }
    mutating func restore(_ next: AnalysisState) { state = next; past.removeAll(); future.removeAll() }
    mutating func updateCurrent(_ next: AnalysisState) { state = next; future.removeAll() }
    mutating func undo() {
        guard let next = past.popLast() else { return }
        future.append(state); state = next
    }
    mutating func redo() {
        guard let next = future.popLast() else { return }
        past.append(state); state = next
    }
}

enum VideoGeometry {
    static func fittedRect(source: CGSize, container: CGSize) -> CGRect {
        guard source.width > 0, source.height > 0 else { return .zero }
        let scale = min(container.width / source.width, container.height / source.height)
        let size = CGSize(width: source.width * scale, height: source.height * scale)
        return CGRect(x: (container.width - size.width) / 2,
                      y: (container.height - size.height) / 2, width: size.width, height: size.height)
    }

    static func sourcePoint(_ point: CGPoint, source: CGSize, rect: CGRect) -> CGPoint? {
        guard rect.width > 0, rect.height > 0, rect.contains(point) else { return nil }
        return CGPoint(x: (point.x - rect.minX) / rect.width * source.width,
                       y: (point.y - rect.minY) / rect.height * source.height)
    }
}

// Positions are ordered by source time. Both manual edits and batch delivery use
// this index so preserving corrections never requires scanning the whole track.
extension PointTrack {
    private func insertionIndex(at time: Double) -> Int {
        var low = 0, high = points.count
        while low < high {
            let middle = (low + high) / 2
            if points[middle].time < time - 0.000001 { low = middle + 1 } else { high = middle }
        }
        return low
    }
    mutating func put(_ point: Measurement, preservingCorrections: Bool = false) {
        if let last = points.last, point.time > last.time + 0.000001 { points.append(point); return }
        let index = insertionIndex(at: point.time)
        if index < points.count, abs(points[index].time - point.time) < 0.000001 {
            if preservingCorrections && points[index].origin != .automatic { return }
            points[index] = point
        } else { points.insert(point, at: index) }
    }
    mutating func mergeAutomatic(_ batch: [Measurement]) {
        guard let first = batch.first else { return }
        if points.isEmpty || first.time > points.last!.time + 0.000001 {
            points.append(contentsOf: batch)
        } else {
            for point in batch { put(point, preservingCorrections: true) }
        }
    }
}
