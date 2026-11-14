import Foundation
import CoreGraphics
import CoreMedia

struct CoordinateSystem: Codable, Equatable, Sendable {
    // Nil preserves the legacy bottom-left origin, including after source loading.
    var origin: CGPoint? = nil
    var angleDegrees = 0.0
    func origin(in size: CGSize) -> CGPoint { origin ?? CGPoint(x: 0, y: size.height) }
    var isValid: Bool {
        angleDegrees.isFinite && abs(angleDegrees) <= 360 &&
        (origin == nil || (origin!.x.isFinite && origin!.y.isFinite))
    }
    func convert(_ pixel: CGPoint, size: CGSize, calibration: Calibration?) -> CGPoint {
        let o = origin(in: size), scale = calibration?.pixelsPerUnit ?? 1
        let dx = (pixel.x-o.x)/scale, dy = (o.y-pixel.y)/scale
        let angle = angleDegrees * .pi/180
        return CGPoint(x: dx*cos(angle)+dy*sin(angle), y: -dx*sin(angle)+dy*cos(angle))
    }
    func isRepresentable(size: CGSize, calibration: Calibration?) -> Bool {
        guard isValid, calibration?.isValid != false else { return false }
        return [CGPoint.zero,CGPoint(x:size.width,y:0),CGPoint(x:0,y:size.height),CGPoint(x:size.width,y:size.height)].allSatisfy {
            let p = convert($0,size:size,calibration:calibration)
            return p.x.isFinite && p.y.isFinite
        }
    }
}

struct MotionSettings: Codable, Equatable, Sendable {
    var window = 3
    var isValid: Bool { [3,5,7,9].contains(window) }
    var method: String { window == 3 ? "three-point-quadratic-v1" : "local-quadratic-v1" }
    var description: String { window == 3 ? "3-point · no smoothing" : "Local quadratic · \(window) points" }
}

enum MotionVariable: String, CaseIterable, Identifiable, Hashable {
    case time = "t", x, y, vx, vy, speed = "v", ax, ay
    var id: String { rawValue }
    var title: String {
        switch self {
        case .time: "t: time"
        case .x: "x: position x-component"
        case .y: "y: position y-component"
        case .vx: "vx: velocity x-component"
        case .vy: "vy: velocity y-component"
        case .speed: "v: speed"
        case .ax: "ax: acceleration x-component"
        case .ay: "ay: acceleration y-component"
        }
    }
    func unit(_ length: String) -> String {
        switch self {
        case .time: "s"
        case .x, .y: length
        case .vx, .vy, .speed: length+"/s"
        case .ax, .ay: length+"/s²"
        }
    }
    func value(_ row: MotionRow) -> Double? {
        switch self {
        case .time: row.sourceTime
        case .x: row.x
        case .y: row.y
        case .vx: row.vx
        case .vy: row.vy
        case .speed: row.speed
        case .ax: row.ax
        case .ay: row.ay
        }
    }
}

struct MotionRow: Identifiable, Equatable {
    var observation: Measurement
    var id: UUID { observation.id }
    var sourceTime: Double { observation.frame?.seconds ?? observation.time }
    var x: Double
    var y: Double
    var vx: Double?
    var vy: Double?
    var ax: Double?
    var ay: Double?
    var unavailableReason: String?
    var speed: Double? { if let vx, let vy { return hypot(vx,vy) }; return nil }
}

enum MotionAnalysis {
    static func calculate(points: [Measurement], sourceTimes: [FrameStamp], size: CGSize,
                          calibration: Calibration?, coordinates: CoordinateSystem, settings: MotionSettings) throws -> [MotionRow] {
        guard settings.isValid, coordinates.isValid else { throw MotionError.invalidSettings }
        guard coordinates.isRepresentable(size:size,calibration:calibration) else { throw MotionError.nonfiniteCoordinates }
        try Task.checkCancellation()
        let half = settings.window/2
        // Binary source lookup distinguishes a missing observation from legitimate VFR timing.
        func sourceIndex(_ point: Measurement) -> Int? {
            var low = 0, high = sourceTimes.count
            if let stamp = point.frame {
                while low < high {
                    let middle = (low+high)/2
                    if CMTimeCompare(sourceTimes[middle].cmTime,stamp.cmTime)<0 { low = middle+1 } else { high = middle }
                }
                return sourceTimes.indices.contains(low) && CMTimeCompare(sourceTimes[low].cmTime,stamp.cmTime)==0 ? low : nil
            }
            let time = point.time
            while low < high {
                let middle = (low+high)/2
                if sourceTimes[middle].seconds <= time+0.0000001 { low = middle+1 } else { high = middle }
            }
            let i = max(0,low-1)
            return sourceTimes.indices.contains(i) && abs(sourceTimes[i].seconds-time)<0.000001 ? i : nil
        }
        var positions: [CGPoint] = [], indices: [Int?] = []
        positions.reserveCapacity(points.count); indices.reserveCapacity(points.count)
        for (i,point) in points.enumerated() {
            if i % 128 == 0 { try Task.checkCancellation() }
            let position = coordinates.convert(point.pixel,size:size,calibration:calibration)
            guard position.x.isFinite, position.y.isFinite else { throw MotionError.nonfiniteCoordinates }
            positions.append(position)
            indices.append(sourceIndex(point))
        }
        var rows: [MotionRow] = []; rows.reserveCapacity(points.count)
        for i in points.indices {
            if i % 128 == 0 { try Task.checkCancellation() }
            var row = MotionRow(observation:points[i],x:positions[i].x,y:positions[i].y)
            if !row.sourceTime.isFinite || !row.x.isFinite || !row.y.isFinite {
                row.unavailableReason = "invalid_observation"
            } else if i < half || i+half >= points.count {
                row.unavailableReason = "insufficient_neighbors"
            } else {
                let range = (i-half)...(i+half)
                var reason: String?
                for j in range {
                    let time = points[j].frame?.seconds ?? points[j].time
                    if !time.isFinite || !positions[j].x.isFinite || !positions[j].y.isFinite { reason = "invalid_observation"; break }
                    if j > range.lowerBound {
                        if time <= (points[j-1].frame?.seconds ?? points[j-1].time) { reason = "invalid_time_interval"; break }
                        guard let a = indices[j-1], let b = indices[j] else { reason = "source_timeline_unavailable"; break }
                        if b != a+1 { reason = "gap_in_observations"; break }
                    }
                }
                if reason == nil {
                    let times = range.map { (points[$0].frame?.seconds ?? points[$0].time)-row.sourceTime }
                    if let x = fit(times:times,values:range.map { positions[$0].x-positions[i].x }),
                       let y = fit(times:times,values:range.map { positions[$0].y-positions[i].y }) {
                        row.vx = x.0; row.ax = x.1; row.vy = y.0; row.ay = y.1
                    } else { reason = "ill_conditioned_window" }
                }
                row.unavailableReason = reason
            }
            rows.append(row)
        }
        return rows
    }

    // Center and scale time, then solve the bounded quadratic fit with QR.
    // Three samples interpolate; wider windows smooth derivatives, never observations.
    private static func fit(times: [Double], values: [Double]) -> (Double, Double)? {
        guard let scale = times.map({abs($0)}).max(), scale.isFinite, scale > 0 else { return nil }
        let u = times.map { $0/scale }
        var columns = [Array(repeating:1.0,count:u.count),u,u.map{$0*$0}]
        var r = Array(repeating:Array(repeating:0.0,count:3),count:3)
        for j in 0..<3 {
            for k in 0..<j {
                let dot = zip(columns[k],columns[j]).reduce(0.0){$0+$1.0*$1.1}
                r[k][j] = dot
                for n in u.indices { columns[j][n] -= dot*columns[k][n] }
            }
            let norm = sqrt(columns[j].reduce(0){$0+$1*$1})
            guard norm.isFinite, norm > 1e-10 else { return nil }
            r[j][j] = norm
            for n in u.indices { columns[j][n] /= norm }
        }
        var c = columns.map { zip($0,values).reduce(0.0){$0+$1.0*$1.1} }
        for j in stride(from:2,through:0,by:-1) {
            if j < 2 { for k in (j+1)..<3 { c[j] -= r[j][k]*c[k] } }
            c[j] /= r[j][j]
        }
        let velocity = c[1]/scale, acceleration = 2*c[2]/scale/scale
        return velocity.isFinite && acceleration.isFinite ? (velocity,acceleration) : nil
    }
}

enum MotionError: LocalizedError {
    case invalidSettings, nonfiniteCoordinates
    var errorDescription: String? {
        switch self {
        case .invalidSettings: "Supply finite axes and a derivative window of 3, 5, 7 or 9 observations."
        case .nonfiniteCoordinates: "The coordinates or scale exceed the supported numerical range."
        }
    }
}

struct MotionKey: Equatable { var revision: Int; var trackID: UUID }

private struct MotionContext: Equatable {
    var sourceTimes: [FrameStamp]
    var size: CGSize
    var calibration: Calibration?
    var coordinates: CoordinateSystem
    var settings: MotionSettings
}

// One bounded cache per session. UI and MCP reuse the same revisioned calculation.
actor MotionCalculator {
    private var key: MotionKey?
    private var context: MotionContext?
    private var cached: [MotionRow] = []
    func rows(key: MotionKey, points: [Measurement], sourceTimes: [FrameStamp], size: CGSize,
              calibration: Calibration?, coordinates: CoordinateSystem, settings: MotionSettings) throws -> [MotionRow] {
        try Task.checkCancellation()
        let context = MotionContext(sourceTimes:sourceTimes,size:size,calibration:calibration,coordinates:coordinates,settings:settings)
        if self.key == key, self.context == context { return cached }
        var appendOnly = self.key?.trackID == key.trackID && self.context == context && !cached.isEmpty && points.count >= cached.count
        if appendOnly {
            for i in cached.indices {
                if i % 128 == 0 { try Task.checkCancellation() }
                if cached[i].observation != points[i] { appendOnly = false; break }
            }
        }
        let result: [MotionRow]
        if appendOnly {
            if points.count == cached.count { self.key = key; return cached }
            let replaceStart = max(0,cached.count-settings.window/2)
            let sliceStart = max(0,replaceStart-settings.window/2)
            let tail = try MotionAnalysis.calculate(points:Array(points[sliceStart...]),sourceTimes:sourceTimes,size:size,
                calibration:calibration,coordinates:coordinates,settings:settings)
            result = Array(cached.prefix(replaceStart))+tail.dropFirst(replaceStart-sliceStart)
        } else {
            result = try MotionAnalysis.calculate(points:points,sourceTimes:sourceTimes,size:size,calibration:calibration,coordinates:coordinates,settings:settings)
        }
        try Task.checkCancellation()
        self.key = key; self.context = context; cached = result
        return result
    }
}
