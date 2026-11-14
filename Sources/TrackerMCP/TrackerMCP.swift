import Foundation
import AppKit
import MCP
import TrackerAutomation

actor AppConnection {
    let clientID = UUID()
    let appURL: URL
    init() throws {
        if let index = CommandLine.arguments.firstIndex(of:"--app"), index+1 < CommandLine.arguments.count {
            let path = CommandLine.arguments[index+1]
            guard path.hasPrefix("/") else { throw AutomationError("invalid_path", "--app requires an absolute path.") }
            appURL = URL(fileURLWithPath:path)
        } else {
            let executable = URL(fileURLWithPath:CommandLine.arguments[0]).resolvingSymlinksInPath()
            appURL = executable.deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        }
    }
    func call(_ name: String, arguments: [String:JSONValue] = [:]) async throws -> JSONValue {
        let request = AutomationRequest(command:name,arguments:arguments,clientID:clientID)
        func send() async throws -> JSONValue {
            try await Task.detached(priority:.utility) {
            let response = try LocalConnection.connect().call(request)
            if let error = response.error { throw error }
            return response.result ?? .null
            }.value
        }
        do { return try await send() }
        catch let error as AutomationError where error.code == "connection_error" && !name.hasPrefix("__") {
            try await connect()
            return try await send()
        }
    }
    func connect() async throws {
        do { _ = try await call("__hello"); return }
        catch let error as AutomationError where error.code != "connection_error" { throw error }
        catch { }
        let running = await MainActor.run { !NSRunningApplication.runningApplications(withBundleIdentifier:"com.imakas.tracker").isEmpty }
        guard !running else { throw AutomationError("app_not_ready", "Tracker is running without a reachable control endpoint. Build/restart the updated app; do not open another instance.") }
        guard appURL.pathExtension == "app", FileManager.default.fileExists(atPath:appURL.appendingPathComponent("Contents/MacOS/Tracker").path) else { throw AutomationError("app_not_found", "Run the bundled TrackerMCP executable, or supply --app /absolute/path/Tracker.app.") }
        let url = appURL
        await MainActor.run {
            let configuration = NSWorkspace.OpenConfiguration(); configuration.arguments = ["--agent-control"]
            NSWorkspace.shared.openApplication(at:url,configuration:configuration) { _, error in
                if let error { FileHandle.standardError.write(Data(("Tracker launch: "+error.localizedDescription+"\n").utf8)) }
            }
        }
        let deadline = Date().addingTimeInterval(15)
        while Date() < deadline {
            try await Task.sleep(for:.milliseconds(100))
            do { _ = try await call("__hello"); return }
            catch let error as AutomationError where error.code != "connection_error" { throw error }
            catch { }
        }
        throw AutomationError("startup_timeout", "Tracker did not expose its local endpoint within 15 seconds. Inspect the app and stderr.")
    }
}

@main
struct TrackerMCP {
    static func main() async {
        do {
            let app = try AppConnection()
            let server = Server(name:"Tracker",version:"0.3.0",instructions:"Control the native Tracker app. Inspect revisions before mutations; use stable IDs and a unique request_id UUID per mutation. Poll returned job_id through get_job. Images use source pixels with top-left origin; image resize/crop mappings must be applied before marking. Calibration requires a known reference. Review candidates are unrecorded until accepted. Match scores are not probabilities.",capabilities:.init(tools:.init(listChanged:false)))
            await server.withMethodHandler(ListTools.self) { _ in
                .init(tools:try ToolCatalog.tools.map { definition in
                    Tool(name:definition.name,description:definition.description,inputSchema:try definition.schema.decode(MCP.Value.self),annotations:.init(readOnlyHint:definition.readOnly,destructiveHint:!definition.readOnly,idempotentHint:true,openWorldHint:false))
                })
            }
            await server.withMethodHandler(CallTool.self) { params in
                do {
                    let args = try JSONValue.from(params.arguments ?? [:]).object ?? [:]
                    var result = try await app.call(params.name,arguments:args)
                    var contents: [Tool.Content] = []
                    if var object = result.object, let data = object.removeValue(forKey:"image_base64")?.string {
                        result = .object(object); contents.append(.image(data:data,mimeType:"image/png",metadata:nil))
                    }
                    contents.insert(.text(try result.text()),at:0)
                    return .init(content:contents,structuredContent:Optional.some(try result.decode(MCP.Value.self)),isError:false)
                } catch {
                    let failure = error as? AutomationError ?? AutomationError("connection_error",error.localizedDescription)
                    let value = try JSONValue.from(failure)
                    return .init(content:[.text(try value.text())],structuredContent:Optional.some(try value.decode(MCP.Value.self)),isError:true)
                }
            }
            try await server.start(transport:CodexStdioTransport(),initializeHook:{ _, _ in try await app.connect() })
            let heartbeat = Task {
                while !Task.isCancelled {
                    try? await Task.sleep(for:.seconds(10))
                    if !Task.isCancelled { _ = try? await app.call("__heartbeat") }
                }
            }
            await server.waitUntilCompleted()
            heartbeat.cancel(); _ = try? await app.call("__disconnect")
        } catch {
            FileHandle.standardError.write(Data(("TrackerMCP: "+error.localizedDescription+"\n").utf8))
            Foundation.exit(1)
        }
    }
}
