import Foundation
import Darwin

// swiftc -O Sources/Tracker/Measurement.swift Sources/Tracker/MotionAnalysis.swift scripts/benchmark-motion.swift -o /tmp/tracker-motion-benchmark
@main struct MotionBenchmark {
    static let size = CGSize(width:1000,height:1000)
    static func input(_ count: Int) -> [Measurement] {
        (0..<count).map { i in
            let t = Double(i)/100
            return Measurement(time:t,pixel:CGPoint(x:500+20*cos(t),y:500-20*sin(t)),
                frame:FrameStamp(value:Int64(i),timescale:100))
        }
    }
    static func elapsed(_ began: ContinuousClock.Instant) -> Double {
        let d = began.duration(to:.now)
        return Double(d.components.seconds)+Double(d.components.attoseconds)/1e18
    }
    static func main() async throws {
        for count in [1_000,10_000,100_000] {
            let points = input(count), stamps = points.compactMap(\.frame)
            for window in [3,7] {
                var times: [Double] = []
                var checksum = 0.0
                for _ in 0..<3 {
                    let began = ContinuousClock.now
                    let rows = try MotionAnalysis.calculate(points:points,sourceTimes:stamps,size:size,
                        calibration:nil,coordinates:CoordinateSystem(),settings:MotionSettings(window:window))
                    times.append(elapsed(began))
                    checksum += rows.reduce(0) { $0+($1.speed ?? 0) }
                }
                print("MOTION_BENCH rows=\(count) window=\(window) median_ms=\(times.sorted()[1]*1000) checksum=\(checksum)")
            }
        }
        let points = input(10_000), stamps = points.compactMap(\.frame)
        for jobs in [1,2,4,8] {
            let began = ContinuousClock.now
            let total = try await withThrowingTaskGroup(of:Int.self) { group in
                for _ in 0..<jobs {
                    group.addTask {
                        try MotionAnalysis.calculate(points:points,sourceTimes:stamps,size:size,
                            calibration:nil,coordinates:CoordinateSystem(),settings:MotionSettings(window:7)).count
                    }
                }
                var n = 0
                for try await count in group { n += count }
                return n
            }
            var usage = rusage(); getrusage(RUSAGE_SELF,&usage)
            print("MOTION_WORKLOAD jobs=\(jobs) rows=\(total) wall_ms=\(elapsed(began)*1000) process_peak_rss_bytes=\(usage.ru_maxrss)")
        }
        let all = input(100_250), sourceTimes = all.compactMap(\.frame)
        var appendTimes: [Double] = []
        for _ in 0..<3 {
            let calculator = MotionCalculator(), track = UUID()
            _ = try await calculator.rows(key:MotionKey(revision:0,trackID:track),points:Array(all.prefix(100_000)),
                sourceTimes:sourceTimes,size:size,calibration:nil,coordinates:CoordinateSystem(),settings:MotionSettings(window:7))
            let began = ContinuousClock.now
            let rows = try await calculator.rows(key:MotionKey(revision:1,trackID:track),points:all,
                sourceTimes:sourceTimes,size:size,calibration:nil,coordinates:CoordinateSystem(),settings:MotionSettings(window:7))
            appendTimes.append(elapsed(began)); precondition(rows.count==100_250)
        }
        print("MOTION_APPEND existing=100000 batch=250 window=7 median_ms=\(appendTimes.sorted()[1]*1000)")
    }
}
