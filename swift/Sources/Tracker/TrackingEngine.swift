import Foundation
import AVFoundation

struct GrayImage {
    let width: Int
    let height: Int
    let pixels: [UInt8]
}

enum MatchingMethod: String, Codable, CaseIterable, Identifiable {
    case squaredDifference = "ssd", normalizedCorrelation = "zncc"
    var id: String { rawValue }
    var title: String { self == .squaredDifference ? "Squared difference" : "Normalized correlation (experimental)" }
    var algorithm: String { self == .squaredDifference ? "fixed-grayscale-ssd-v1" : "fixed-grayscale-zncc-v1" }
}

struct TrackingSettings: Codable, Equatable {
    var templateRadius = 12
    var searchRadius = 24
    var minimumScore = 0.90
    var minimumSeparation = 0.015
    var matchingMethod: MatchingMethod = .squaredDifference
    var isValid: Bool {
        (3...48).contains(templateRadius) && (4...128).contains(searchRadius) &&
        minimumScore.isFinite && (0...1).contains(minimumScore) &&
        minimumSeparation.isFinite && (0...1).contains(minimumSeparation)
    }
}

extension TrackingSettings {
    private enum CodingKeys: String, CodingKey { case templateRadius, searchRadius, minimumScore, minimumSeparation, matchingMethod }
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        templateRadius = try c.decode(Int.self, forKey: .templateRadius)
        searchRadius = try c.decode(Int.self, forKey: .searchRadius)
        minimumScore = try c.decode(Double.self, forKey: .minimumScore)
        minimumSeparation = try c.decode(Double.self, forKey: .minimumSeparation)
        matchingMethod = try c.decodeIfPresent(MatchingMethod.self, forKey: .matchingMethod) ?? .squaredDifference
    }
}

struct MatchEvidence {
    var position: CGPoint?
    var score: Double
    var separation: Double
    var reason: String?
    var accepted: Bool { position != nil && reason == nil }
}

enum TrackingError: LocalizedError {
    case invalidTemplate, invalidSettings
    var errorDescription: String? {
        switch self {
        case .invalidTemplate: "The template needs visible detail and must fit inside the video. Enlarge it to include an edge, or choose another point."
        case .invalidSettings: "Tracking settings are outside the supported range."
        }
    }
}

struct TemplateMatcher {
    let radius: Int
    let template: [UInt8]
    let centeredTemplate: [Double]
    let templateEnergy: Double
    let settings: TrackingSettings
    init(image: GrayImage, center: CGPoint, settings: TrackingSettings) throws {
        guard settings.isValid else { throw TrackingError.invalidSettings }
        self.settings = settings; radius = settings.templateRadius
        let x = Int(center.x.rounded()), y = Int(center.y.rounded())
        guard x >= radius, y >= radius, x + radius < image.width, y + radius < image.height,
              image.pixels.count == image.width * image.height else { throw TrackingError.invalidTemplate }
        var patch: [UInt8] = []
        for row in (y-radius)...(y+radius) {
            patch.append(contentsOf: image.pixels[(row*image.width+x-radius)...(row*image.width+x+radius)])
        }
        let mean = patch.reduce(0.0) { $0 + Double($1) } / Double(patch.count)
        let variance = patch.reduce(0.0) { $0 + pow(Double($1) - mean, 2) } / Double(patch.count)
        guard variance > 16 else { throw TrackingError.invalidTemplate }
        template = patch
        centeredTemplate = patch.map { Double($0) - mean }
        templateEnergy = variance * Double(patch.count)
    }

    func match(_ image: GrayImage, around center: CGPoint) throws -> MatchEvidence {
        guard image.pixels.count == image.width * image.height else { throw MediaError.decode }
        let cx = Int(center.x.rounded()), cy = Int(center.y.rounded())
        let x0 = max(radius, cx - settings.searchRadius), x1 = min(image.width-radius-1, cx + settings.searchRadius)
        let y0 = max(radius, cy - settings.searchRadius), y1 = min(image.height-radius-1, cy + settings.searchRadius)
        guard x0 <= x1, y0 <= y1 else {
            return MatchEvidence(position: nil, score: 0, separation: 0, reason: "Search region is outside the image.")
        }
        func cost(_ x: Int, _ y: Int) -> Double {
            if settings.matchingMethod == .normalizedCorrelation {
                var sum = 0.0, squares = 0.0, dot = 0.0, index = 0
                for row in (y-radius)...(y+radius) {
                    let base = row * image.width + x - radius
                    for column in 0..<(2*radius+1) {
                        let value = Double(image.pixels[base+column])
                        sum += value; squares += value*value
                        dot += value * centeredTemplate[index]; index += 1
                    }
                }
                let energy = max(0, squares - sum*sum/Double(template.count))
                // Flat candidates cannot establish a normalized match.
                guard energy > 16 * Double(template.count) else { return 2 }
                return 1 - max(-1, min(1, dot / sqrt(templateEnergy * energy)))
            }
            var sum = 0.0, index = 0
            for row in (y-radius)...(y+radius) {
                let base = row * image.width + x - radius
                for column in 0..<(2*radius+1) {
                    let difference = Double(image.pixels[base+column]) - Double(template[index])
                    sum += difference*difference; index += 1
                }
            }
            return sum / (Double(template.count) * 255 * 255)
        }
        var costs: [Double] = []
        costs.reserveCapacity((x1-x0+1)*(y1-y0+1))
        var best = Double.infinity, bx = x0, by = y0
        for y in y0...y1 {
            try Task.checkCancellation()
            for x in x0...x1 {
                let value = cost(x, y); costs.append(value)
                if value < best { best = value; bx = x; by = y }
            }
        }
        var alternative = Double.infinity
        let exclusion = max(2, radius / 2)
        for y in y0...y1 {
            for x in x0...x1 where abs(x-bx) > exclusion || abs(y-by) > exclusion {
                alternative = min(alternative, costs[(y-y0)*(x1-x0+1)+x-x0])
            }
        }
        let score = max(0, 1-best)
        let margin = alternative.isFinite ? max(0, alternative-best) : 0
        func offset(_ before: Double, _ after: Double) -> Double {
            let denominator = before - 2*best + after
            return denominator > 0.00000001 ? max(-0.5, min(0.5, 0.5*(before-after)/denominator)) : 0
        }
        let dx = bx > x0 && bx < x1 ? offset(cost(bx-1, by), cost(bx+1, by)) : 0
        let dy = by > y0 && by < y1 ? offset(cost(bx, by-1), cost(bx, by+1)) : 0
        let reason: String?
        if score < settings.minimumScore { reason = "Appearance differs from the template." }
        else if margin < settings.minimumSeparation { reason = "Competing matches have similar scores." }
        else if bx == x0 || bx == x1 || by == y0 || by == y1 { reason = "Best match is at the search boundary." }
        else { reason = nil }
        return MatchEvidence(position: CGPoint(x: Double(bx)+dx, y: Double(by)+dy), score: score, separation: margin, reason: reason)
    }
}

struct TrackingReview {
    let frame: DecodedFrame
    let evidence: MatchEvidence
}

enum TrackingEvent {
    case update([Measurement], Double, Int)
    case review([Measurement], TrackingReview, Int)
    case complete([Measurement], Int, Double)
}

enum TrackingEngine {
    static func run(url: URL, seed: Measurement, settings: TrackingSettings, after: Double?, clip: VideoClip? = nil, sourceTimes: [FrameStamp] = [],
                    publish: @Sendable (TrackingEvent) async -> Void) async throws {
        let started = Date()
        try Task.checkCancellation()
        let selected = try clip?.timestamps(in:sourceTimes)
        if let selected, !selected.contains(where: { abs($0.seconds-(seed.frame?.seconds ?? seed.time)) < 0.000001 }) { throw ClipError.invalidBounds }
        let media = MediaSource(url: url)
        let seedFrame: DecodedFrame
        if let stamp = seed.frame { seedFrame = try await media.image(at: stamp) }
        else { seedFrame = try await media.image(at: seed.time) }
        guard abs(seedFrame.stamp.seconds - seed.time) < 0.000001 else { throw MediaError.decode }
        let matcher = try TemplateMatcher(image: seedFrame.grayscale(), center: seed.pixel, settings: settings)
        let end = clip.flatMap { $0.lastSampleFrame+1 < sourceTimes.count ? sourceTimes[$0.lastSampleFrame+1].cmTime : nil }
        let stream = try await SequentialFrames(url: url, start: seedFrame.stamp.cmTime, endExclusive:end)
        defer { stream.cancel() }
        var previous = seed.pixel
        var batch: [Measurement] = []
        var count = 0
        var lastPublish = Date()
        var selectedIndex = 0
        let sampleTimes = (clip?.stepSize ?? 1) > 1 ? selected.map { Set($0.map(\.seconds)) } : nil
        while let frame = try autoreleasepool(invoking: { try stream.next(including: { sampleTimes?.contains($0.seconds) ?? true }) }) {
            try Task.checkCancellation()
            if let selected {
                while selectedIndex < selected.count, CMTimeCompare(selected[selectedIndex].cmTime,frame.stamp.cmTime) < 0 { selectedIndex += 1 }
                if selectedIndex == selected.count { break }
                if CMTimeCompare(selected[selectedIndex].cmTime,frame.stamp.cmTime) != 0 { continue }
            }
            if frame.stamp.seconds <= max(seed.time, after ?? seed.time) { continue }
            let evidence = try matcher.match(frame.grayscale(), around: previous)
            count += 1
            guard evidence.accepted, let position = evidence.position else {
                await publish(.review(batch, TrackingReview(frame: frame, evidence: evidence), count))
                return
            }
            batch.append(Measurement(time: frame.stamp.seconds, pixel: position, origin: .automatic, score: evidence.score, frame: frame.stamp, matchingMethod: settings.matchingMethod))
            previous = position
            if Date().timeIntervalSince(lastPublish) >= 0.1 {
                await publish(.update(batch, frame.stamp.seconds, count)); batch.removeAll(keepingCapacity: true)
                lastPublish = Date()
            }
        }
        await publish(.complete(batch, count, Date().timeIntervalSince(started)))
    }
}
