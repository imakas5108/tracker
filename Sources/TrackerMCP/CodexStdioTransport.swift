import Foundation
import MCP
import Logging
import TrackerAutomation

actor CodexStdioTransport: Transport {
    let logger = Logger(label:"com.imakas.tracker.mcp")
    private let stdio = StdioTransport()
    func connect() async throws { try await stdio.connect() }
    func disconnect() async { await stdio.disconnect() }
    func send(_ data: Data) async throws { try await stdio.send(data) }
    func receive() -> AsyncThrowingStream<Data, Swift.Error> {
        AsyncThrowingStream { continuation in
            let task = Task {
                do {
                    for try await data in await stdio.receive() {
                        continuation.yield(MCPInitialization.compatibleMessage(data))
                    }
                    continuation.finish()
                } catch { continuation.finish(throwing:error) }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }
}
