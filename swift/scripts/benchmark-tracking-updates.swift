import Foundation

// Compile with Sources/Tracker/Measurement.swift using -O. The reference loop is
// the update logic at f5d5470; both paths preserve the same correction policy.
@main struct UpdateBenchmark {
    @MainActor static func main() {
        for size in [1_000, 10_000, 100_000] {
            let initial = PointTrack(name:"Synthetic update benchmark",points:(0..<size).map { Measurement(time:Double($0),pixel:.zero,origin:.automatic) })
            let batch = (size..<(size+250)).map { Measurement(time:Double($0),pixel:CGPoint(x:1,y:1),origin:.automatic) }
            var oldTimes: [Double] = [], newTimes: [Double] = []
            for _ in 0..<5 {
                var old = initial
                var began = ContinuousClock.now
                for point in batch {
                    if let previous = old.points.first(where:{abs($0.time-point.time)<0.000001}),previous.origin != .automatic {continue}
                    old.put(point)
                }
                oldTimes.append(seconds(began.duration(to:.now)))
                var improved = initial; began = .now; improved.mergeAutomatic(batch)
                newTimes.append(seconds(began.duration(to:.now)))
                precondition(old.points==improved.points)
            }
            print("UPDATE_BENCH existing=\(size) batch=250 old_median_ms=\(oldTimes.sorted()[2]*1000) new_median_ms=\(newTimes.sorted()[2]*1000)")
        }
    }
    static func seconds(_ duration: Duration) -> Double { Double(duration.components.seconds)+Double(duration.components.attoseconds)/1e18 }
}
