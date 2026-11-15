import Foundation

// Baseline execution budget: queue jobs, never limit the number of video sessions.
// Increase parallel decoder/CPU admission only after the benchmark supports it.
actor TrackingCapacity {
    static let shared = TrackingCapacity()
    private var owner: UUID?
    private var waiting: [(UUID, CheckedContinuation<Void, Error>)] = []
    var queuedCount: Int { waiting.count }
    func acquire(_ id: UUID) async throws {
        try Task.checkCancellation()
        if owner == nil { owner = id; return }
        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                waiting.append((id, continuation))
                if Task.isCancelled { cancelWait(id) }
            }
        } onCancel: { Task { await self.cancelWait(id) } }
        if Task.isCancelled { release(id); throw CancellationError() }
    }
    private func cancelWait(_ id: UUID) {
        if let index = waiting.firstIndex(where: { $0.0 == id }) {
            let entry = waiting.remove(at: index); entry.1.resume(throwing: CancellationError())
        }
    }
    func release(_ id: UUID) {
        guard owner == id else { return }
        if waiting.isEmpty { owner = nil }
        else { let next = waiting.removeFirst(); owner = next.0; next.1.resume() }
    }
}
