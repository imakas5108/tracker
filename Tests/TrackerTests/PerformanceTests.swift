import XCTest
import Darwin
@testable import Tracker

final class PerformanceTests: XCTestCase {
    private var clip: URL {
        URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("build/fixtures/clean.mov")
    }
    func testQueuedWorkloadAndResourceBaseline() async throws {
        guard FileManager.default.fileExists(atPath: clip.path) else { throw XCTSkip("Generate synthetic fixtures first") }
        for jobs in [1, 2, 4] {
            let capacity = TrackingCapacity()
            let source = clip
            let before = usage()
            let start = Date()
            let total = try await withThrowingTaskGroup(of: Int.self) { group in
                for _ in 0..<jobs {
                    group.addTask {
                        let id = UUID()
                        try await capacity.acquire(id)
                        let count = FrameCount()
                        do {
                            try await TrackingEngine.run(url: source,
                                seed: Measurement(time: 0, pixel: CGPoint(x: 30, y: 45)),
                                settings: TrackingSettings(templateRadius: 8), after: nil) { await count.record($0) }
                        } catch { await capacity.release(id); throw error }
                        await capacity.release(id)
                        return await count.value
                    }
                }
                var count = 0
                for try await value in group { count += value }
                return count
            }
            let elapsed = Date().timeIntervalSince(start), after = usage()
            XCTAssertEqual(total, jobs*59)
            print("TRACKER_RESOURCE jobs=\(jobs) active_budget=1 frames=\(total) wall_s=\(elapsed) cpu_s=\(after.cpu-before.cpu) process_peak_rss_bytes=\(after.rss) aggregate_fps=\(Double(total)/elapsed)")
        }
    }
    func testRunningEngineCancellationCompletes() async throws {
        guard FileManager.default.fileExists(atPath: clip.path) else { throw XCTSkip("Generate synthetic fixtures first") }
        let source = clip
        let started = EngineStartSignal()
        let task = Task.detached {
            do {
                try await TrackingEngine.run(url: source, seed: Measurement(time: 0, pixel: CGPoint(x: 30, y: 45)),
                    settings: TrackingSettings(templateRadius: 8, searchRadius: 100), after: nil) { _ in await started.signal() }
            } catch {
                await started.signal()
                throw error
            }
        }
        await started.wait()
        let start = Date(); task.cancel()
        do { try await task.value; XCTFail("Expected cancellation before the full workload completed") }
        catch is CancellationError {} catch { XCTFail("Unexpected error: \(error)") }
        let elapsed = Date().timeIntervalSince(start)
        XCTAssertLessThan(elapsed, 3)
        print("TRACKER_CANCEL completion_s=\(elapsed)")
    }
    private func usage() -> (cpu: Double, rss: Int) {
        var value = rusage(); getrusage(RUSAGE_SELF, &value)
        let user = Double(value.ru_utime.tv_sec)+Double(value.ru_utime.tv_usec)/1_000_000
        let system = Double(value.ru_stime.tv_sec)+Double(value.ru_stime.tv_usec)/1_000_000
        return (user+system, Int(value.ru_maxrss))
    }
}

private actor FrameCount {
    var value = 0
    func record(_ event: TrackingEvent) {
        switch event {
        case .update(let points, _, _), .complete(let points, _, _), .review(let points, _, _): value += points.count
        }
    }
}

private actor EngineStartSignal {
    private var started = false
    private var waiter: CheckedContinuation<Void, Never>?
    func signal() { started = true; waiter?.resume(); waiter = nil }
    func wait() async {
        if started { return }
        await withCheckedContinuation { waiter = $0 }
    }
}
