import Foundation

struct ProjectFile: Codable {
    var version = 1
    var source: URL
    var sourceSize: CGSize
    var analysis: AnalysisState
    var time: Double
    var settings: TrackingSettings
    var algorithm = "fixed-grayscale-ssd-v1"

    func validated() throws -> Self {
        guard [1,2,3,4].contains(version) else { throw ProjectError.unsupportedVersion }
        if let clip = analysis.clip {
            guard version >= 4 else { throw ProjectError.invalidData }
            _ = try clip.validated()
        }
        guard settings.matchingMethod == .squaredDifference || version >= 3,
              version < 3 || algorithm == settings.matchingMethod.algorithm else { throw ProjectError.invalidData }
        guard source.isFileURL, sourceSize.width.isFinite, sourceSize.height.isFinite,
              sourceSize.width > 0, sourceSize.height > 0, time.isFinite, time >= 0,
              settings.isValid,
              (analysis.coordinates ?? CoordinateSystem()).isRepresentable(size:sourceSize,calibration:analysis.calibration),
              analysis.calibration.map({ ["m","cm","mm"].contains($0.unit) }) != false,
              analysis.motion?.isValid != false else { throw ProjectError.invalidData }
        let trackIDs = analysis.tracks.map(\.id)
        guard Set(trackIDs).count == trackIDs.count,
              analysis.selectedTrackID == nil || trackIDs.contains(analysis.selectedTrackID!),
              analysis.calibration == nil || analysis.calibration!.isValid else { throw ProjectError.invalidData }
        for track in analysis.tracks {
            guard Set(track.points.map(\.id)).count == track.points.count else { throw ProjectError.invalidData }
            var last = -Double.infinity
            for point in track.points {
                guard point.matchingMethod != .normalizedCorrelation || version >= 3 else { throw ProjectError.invalidData }
                guard point.time.isFinite, point.time >= 0, point.time > last,
                      point.pixel.x.isFinite, point.pixel.y.isFinite,
                      point.pixel.x >= 0, point.pixel.x < sourceSize.width,
                      point.pixel.y >= 0, point.pixel.y < sourceSize.height,
                      point.score == nil || (point.score!.isFinite && (0...1).contains(point.score!)),
                      point.frame == nil || (point.frame!.timescale > 0 && abs(point.frame!.seconds - point.time) < 0.000001)
                else { throw ProjectError.invalidData }
                last = point.time
            }
        }
        return self
    }
    static func read(_ url: URL) throws -> Self {
        try JSONDecoder().decode(Self.self, from: Data(contentsOf: url)).validated()
    }
    func write(_ url: URL) throws {
        let data = try JSONEncoder().encode(validated())
        try data.write(to: url, options: .atomic)
    }
}

enum ProjectError: LocalizedError {
    case unsupportedVersion, invalidData, sourceMismatch
    var errorDescription: String? {
        switch self {
        case .unsupportedVersion: "This project version is not supported."
        case .invalidData: "This project contains invalid measurements or settings."
        case .sourceMismatch: "The video dimensions do not match the saved project. Relink the original video."
        }
    }
}
