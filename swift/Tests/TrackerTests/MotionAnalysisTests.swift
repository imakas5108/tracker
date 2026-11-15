import Testing
import Foundation
@testable import Tracker

struct MotionAnalysisTests {
    private let size = CGSize(width:1000,height:1000)
    private func points(_ times: [Double], position: (Double)->CGPoint) -> [Tracker.Measurement] {
        times.map { Measurement(time:$0,pixel:position($0),frame:FrameStamp(value:Int64(($0*1_000_000).rounded()),timescale:1_000_000)) }
    }
    private func calculate(_ p: [Tracker.Measurement], window: Int = 3, timeline: [FrameStamp]? = nil) throws -> [MotionRow] {
        try MotionAnalysis.calculate(points:p,sourceTimes:timeline ?? p.compactMap(\.frame),size:size,calibration:nil,
            coordinates:CoordinateSystem(),settings:MotionSettings(window:window))
    }
    @Test func axesFollowCounterclockwiseConventionAndCalibration() {
        let scale = Calibration(first:.zero,second:CGPoint(x:100,y:0),length:1,unit:"m")
        let axes = CoordinateSystem(origin:CGPoint(x:200,y:500),angleDegrees:90)
        let p = axes.convert(CGPoint(x:200,y:400),size:size,calibration:scale)
        #expect(abs(p.x-1)<1e-12); #expect(abs(p.y)<1e-12)
        let q = axes.convert(CGPoint(x:300,y:500),size:size,calibration:scale)
        #expect(abs(q.x)<1e-12); #expect(abs(q.y+1)<1e-12)
        #expect(CoordinateSystem().convert(CGPoint(x:20,y:30),size:size,calibration:nil) == CGPoint(x:20,y:970))
        let tinyScale = Calibration(first:.zero,second:CGPoint(x:1,y:0),length:1e308,unit:"m")
        #expect(!CoordinateSystem().isRepresentable(size:size,calibration:tinyScale))
    }
    @Test(arguments:[3,5,7,9]) func quadraticAtIrregularTimestamps(window: Int) throws {
        let times = [0.0,0.04,0.11,0.15,0.24,0.31,0.33,0.42,0.51,0.56,0.69,0.73,0.8]
        let input = points(times) { t in CGPoint(x:100+3*t+2*t*t,y:500-7*t+5*t*t) }
        let rows = try calculate(input,window:window)
        for i in window/2..<(rows.count-window/2) {
            let vx = try #require(rows[i].vx), vy = try #require(rows[i].vy)
            let ax = try #require(rows[i].ax), ay = try #require(rows[i].ay)
            #expect(abs(vx-(3+4*times[i]))<1e-8); #expect(abs(vy-(7-10*times[i]))<1e-8)
            #expect(abs(ax-4)<1e-7); #expect(abs(ay+10)<1e-7)
            #expect(rows[i].observation == input[i])
        }
        #expect(rows.first?.unavailableReason == "insufficient_neighbors")
        #expect(rows.last?.vx == nil)
    }
    @Test(arguments:[0.0,5.0]) func stationaryAndConstantVelocity(speed: Double) throws {
        let times = (0..<10).map { Double($0)/20 }
        let rows = try calculate(points(times) { CGPoint(x:50+speed*$0,y:300) })
        #expect(abs(try #require(rows[4].vx)-speed)<1e-10)
        #expect(abs(try #require(rows[4].ax))<1e-9)
        #expect(abs(try #require(rows[4].speed)-speed)<1e-10)
    }
    @Test func circularMotionHasCentripetalAcceleration() throws {
        let times = (0...20).map { Double($0)/1000 }
        let rows = try calculate(points(times) { t in CGPoint(x:500+20*cos(3*t),y:500-20*sin(3*t)) })
        let middle = rows[10], t = times[10]
        #expect(abs(try #require(middle.vx)+60*sin(3*t))<0.0001)
        #expect(abs(try #require(middle.ay)+180*sin(3*t))<0.0001)
        #expect(abs(try #require(middle.speed)-60)<0.0001)
    }
    @Test func gapsAreNotInterpolatedAndBadIntervalsAreUnavailable() throws {
        let times = (0..<9).map { Double($0)/10 }
        let all = points(times) { CGPoint(x:100+$0,y:200) }
        var missing = all; missing.remove(at:4)
        let rows = try calculate(missing,timeline:all.compactMap(\.frame))
        #expect(rows[3].unavailableReason == "gap_in_observations")
        #expect(rows[4].vx == nil)
        #expect(rows[1].vx != nil)
        var duplicate = all; duplicate[4] = all[3]
        #expect(try calculate(duplicate)[4].unavailableReason == "invalid_time_interval")
        #expect(try calculate(Array(all.prefix(2))).allSatisfy { $0.vx == nil })
        #expect(try calculate(all,timeline:[])[3].unavailableReason == "source_timeline_unavailable")
    }
    @Test func rationalStampsTakePrecedenceOverRoundedLegacySeconds() throws {
        let stamps = (0..<5).map { FrameStamp(value:Int64($0),timescale:30) }
        let input = stamps.map { stamp in
            Tracker.Measurement(time:(stamp.seconds*1e6).rounded()/1e6,pixel:CGPoint(x:100+3*stamp.seconds,y:500),frame:stamp)
        }
        let rows = try calculate(input,timeline:stamps)
        #expect(rows[1].sourceTime == stamps[1].seconds)
        #expect(abs(try #require(rows[1].vx)-3)<1e-9)
        #expect(abs(try #require(rows[1].ax))<1e-8)
    }
    @Test func smoothingReducesDerivativeNoiseWithoutChangingPositions() throws {
        let times = (0..<41).map { Double($0)/10 }
        let raw = points(times) { t in
            let i = Int((t*10).rounded())
            return CGPoint(x:200+2*t+(i%2 == 0 ? 0.1 : -0.1),y:500)
        }
        let plain = try calculate(raw), smooth = try calculate(raw,window:7)
        let plainError = plain[4..<37].reduce(0) { $0+pow(($1.ax ?? 0),2) }
        let smoothError = smooth[4..<37].reduce(0) { $0+pow(($1.ax ?? 0),2) }
        #expect(smoothError < plainError/10)
        #expect(smooth.map(\.observation) == raw)
        #expect(smooth.map(\.x) == plain.map(\.x))
    }
    @Test func projectMigrationAndExportDescribeProcessing() throws {
        let input = points([0,0.1,0.2]) { CGPoint(x:100+2*$0,y:500) }
        let track = PointTrack(name:"Motion",points:input)
        var file = ProjectFile(source:URL(fileURLWithPath:"/tmp/synthetic.mov"),sourceSize:size,
            analysis:AnalysisState(tracks:[track]),time:0,settings:TrackingSettings())
        let legacy = try JSONDecoder().decode(ProjectFile.self,from:JSONEncoder().encode(file))
        #expect(legacy.analysis.coordinates == nil); #expect(legacy.analysis.motion == nil)
        file.version = 2; file.analysis.coordinates = CoordinateSystem(origin:CGPoint(x:1,y:2),angleDegrees:30)
        file.analysis.motion = MotionSettings(window:5)
        let reopened = try JSONDecoder().decode(ProjectFile.self,from:JSONEncoder().encode(file)).validated()
        #expect(reopened.analysis == file.analysis)
        file.analysis.calibration = Calibration(first:.zero,second:CGPoint(x:100,y:0),length:1,unit:"unsupported")
        #expect(throws:(any Error).self) { try file.validated() }
        file.analysis.calibration = nil
        file.analysis.motion = MotionSettings(window:4)
        #expect(throws:(any Error).self) { try file.validated() }
        let rows = try calculate(input)
        let csv = String(decoding:try MotionOutput.csv(rows,unit:"px",coordinates:CoordinateSystem(),size:size,settings:MotionSettings()),as:UTF8.self)
        #expect(csv.contains("vx_px_s")); #expect(csv.contains("insufficient_neighbors")); #expect(csv.contains("three-point-quadratic-v1"))
        let json = try MotionOutput.json(rows[1],unit:"px")
        #expect(abs(try #require(json["vx"].number)-2)<1e-9)
        #expect(json["velocity_unit"].string == "px/s")
    }
    @Test func correctionsInvalidateDerivedCacheWithoutReplacingObservations() async throws {
        let input = points([0,0.1,0.2,0.3,0.4]) { CGPoint(x:100+2*$0,y:500) }
        let calculator = MotionCalculator(), track = UUID()
        let original = try await calculator.rows(key:MotionKey(revision:0,trackID:track),points:input,
            sourceTimes:input.compactMap(\.frame),size:size,calibration:nil,coordinates:CoordinateSystem(),settings:MotionSettings())
        var corrected = input
        corrected[2] = Tracker.Measurement(time:0.2,pixel:CGPoint(x:101.4,y:500),frame:input[2].frame)
        let result = try await calculator.rows(key:MotionKey(revision:1,trackID:track),points:corrected,
            sourceTimes:input.compactMap(\.frame),size:size,calibration:nil,coordinates:CoordinateSystem(),settings:MotionSettings())
        #expect(abs(try #require(original[1].vx)-2)<1e-9)
        #expect(abs(try #require(result[1].vx)-7)<1e-9)
        #expect(abs(try #require(result[2].ax)+200)<1e-7)
        #expect(result.map(\.observation) == corrected)
    }
    @Test @MainActor func cancelledCalculationCannotPopulateTheCache() async throws {
        let input = points([0,0.1,0.2]) { CGPoint(x:100+$0,y:500) }
        let calculator = MotionCalculator(), key = MotionKey(revision:0,trackID:UUID())
        let task = Task {
            try await calculator.rows(key:key,points:input,sourceTimes:input.compactMap(\.frame),size:size,
                calibration:nil,coordinates:CoordinateSystem(),settings:MotionSettings())
        }
        task.cancel()
        await #expect(throws:CancellationError.self) { try await task.value }
        let retry = try await calculator.rows(key:key,points:input,sourceTimes:input.compactMap(\.frame),size:size,
            calibration:nil,coordinates:CoordinateSystem(),settings:MotionSettings())
        #expect(abs(try #require(retry[1].vx)-1)<1e-9)
    }
    @Test(arguments:[3,5,7,9]) func appendCacheMatchesFullRecalculationAcrossGaps(window: Int) async throws {
        let all = points((0..<25).map { Double($0)/30 }) { CGPoint(x:100+3*$0,y:500-2*$0*$0) }
        var growing: [Tracker.Measurement] = []
        let calculator = MotionCalculator(), track = UUID(), settings = MotionSettings(window:window)
        for i in all.indices where i != 13 {
            growing.append(all[i])
            let incremental = try await calculator.rows(key:MotionKey(revision:i,trackID:track),points:growing,
                sourceTimes:all.compactMap(\.frame),size:size,calibration:nil,coordinates:CoordinateSystem(),settings:settings)
            let full = try calculate(growing,window:window,timeline:all.compactMap(\.frame))
            #expect(incremental == full)
        }
    }
}
