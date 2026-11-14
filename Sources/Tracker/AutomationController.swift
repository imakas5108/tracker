import Foundation
import Combine
import TrackerAutomation

@MainActor
final class AutomationOperation {
    let id: UUID
    let kind: String
    var state = "queued"
    var result: JSONValue = .null
    var error: AutomationError?
    var session: VideoSession?
    var trackingID: UUID?
    var task: Task<Void, Never>?
    var cancelAction: (() -> Void)?
    init(id: UUID = UUID(), kind: String, session: VideoSession? = nil, trackingID: UUID? = nil) {
        self.id = id; self.kind = kind; self.session = session; self.trackingID = trackingID
    }
    func snapshot() throws -> JSONValue {
        var object: [String: JSONValue] = ["job_id": .string(id.uuidString), "kind": .string(kind), "state": .string(state), "result": result]
        if let session {
            object["session_id"] = .string(session.id.uuidString); object["revision"] = .number(Double(session.revision))
            if let trackingID, let record = session.trackingJobs[trackingID] {
                object["state"] = .string(record.state.lowercased().replacingOccurrences(of:" ",with:"_"))
                object["track_id"] = .string(record.trackID.uuidString); object["processed_frames"] = .number(Double(record.processed))
                if let message = record.error { object["error"] = .object(["code":.string("tracking_failed"),"message":.string(message)]) }
            }
        }
        if let error { object["error"] = try .from(error) }
        return .object(object)
    }
}

@MainActor
final class AutomationController: ObservableObject {
    unowned let workspace: Workspace
    @Published private(set) var enabled = false
    @Published private(set) var clients: [UUID: Date] = [:]
    @Published private(set) var actions: [String] = []
    @Published private(set) var endpointError: String?
    private var server: LocalSocketServer?
    private var leaseTask: Task<Void, Never>?
    private var generation = UUID()
    private var requests: [UUID: (String, AutomationResponse)] = [:]
    private(set) var operations: [UUID: AutomationOperation] = [:]
    init(workspace: Workspace) { self.workspace = workspace }
    func start(enabled: Bool) {
        guard server == nil else { return }
        self.enabled = enabled
        let listener = LocalSocketServer()
        do {
            try listener.start { [weak self] request in
                guard let self else { return AutomationResponse(error: AutomationError("app_shutdown", "Tracker is shutting down.")) }
                return await self.handle(request)
            }
            server = listener
            leaseTask = Task { [weak self] in
                while !Task.isCancelled {
                    try? await Task.sleep(for: .seconds(10))
                    guard let self, !Task.isCancelled else { return }
                    self.clients = self.clients.filter { Date().timeIntervalSince($0.value) < 30 }
                }
            }
        } catch { endpointError = error.localizedDescription }
    }
    func stopControl() { enabled = false; generation = UUID(); clients.removeAll(); log("User stopped agent control") }
    func allowControl() { enabled = true; log("User allowed agent control") }
    func shutdown() {
        stopControl(); server?.stop(); server = nil; leaseTask?.cancel()
        for operation in operations.values where operation.trackingID == nil { operation.task?.cancel() }
    }
    private func log(_ message: String) {
        actions.insert(Date().formatted(date:.omitted,time:.standard)+" · "+message,at:0)
        if actions.count > 30 { actions.removeLast(actions.count-30) }
    }
    func handle(_ request: AutomationRequest) async -> AutomationResponse {
        do {
            guard request.version == 1 else { throw AutomationError("incompatible_protocol", "Tracker requires local automation protocol 1.") }
            if request.command == "__disconnect" { clients.removeValue(forKey: request.clientID); return AutomationResponse(result:.object(["disconnected":.bool(true)])) }
            guard enabled else { throw AutomationError("control_disabled", "Agent control is disabled. Allow Agent Control in Tracker to reconnect.") }
            clients[request.clientID] = Date()
            if request.command == "__hello" || request.command == "__heartbeat" {
                return AutomationResponse(result:.object(["protocol_version":.number(1),"app_version":.string("0.3.0"),"control_enabled":.bool(true)]))
            }
            let definition = try ToolCatalog.validate(request)
            let arguments = request.arguments
            if !definition.readOnly {
                let requestID = try arguments.requiredID("request_id")
                let fingerprint = try JSONValue.object(["command":.string(request.command),"arguments":.object(arguments)]).text()
                if let previous = requests[requestID] {
                    guard previous.0 == fingerprint else { throw AutomationError("request_id_conflict", "This request_id already identifies different arguments.") }
                    return previous.1
                }
                // Mutations reserve their ID without suspension on the main actor.
                let response: AutomationResponse
                do { response = AutomationResponse(result: try mutate(request.command, arguments)) }
                catch { response = AutomationResponse(error: failure(error)) }
                requests[requestID] = (fingerprint,response)
                log(actionDescription(request.command, arguments) + (response.error.map { " · "+$0.code } ?? ""))
                return response
            }
            let epoch = generation
            let value = try await read(request.command,arguments)
            guard enabled, epoch == generation else { throw AutomationError("control_disabled", "Agent control was stopped during this request.") }
            if request.command == "tracker_get_frame" { log("Inspected frame · " + (arguments["session_id"]?.string.map { String($0.prefix(8)) } ?? "") + " · " + String(value["time"].number ?? 0) + " s") }
            return AutomationResponse(result:value)
        } catch { return AutomationResponse(error: failure(error)) }
    }
    private func actionDescription(_ command: String, _ args: [String:JSONValue]) -> String {
        let sessionName = args["session_id"]?.string.flatMap { id in workspace.sessions.first { $0.id.uuidString == id.uppercased() }?.name }
        let label: String
        switch command {
        case "tracker_import_video": label = "Opened video " + (args["path"]?.string.map { URL(fileURLWithPath:$0).lastPathComponent } ?? "")
        case "tracker_open_project": label = "Requested project open " + (args["path"]?.string.map { URL(fileURLWithPath:$0).lastPathComponent } ?? "")
        case "tracker_select_session": label = "Selected video"
        case "tracker_seek": label = "Moved viewer to \(args["time"]?.number ?? 0) s"
        case "tracker_add_track": label = "Added track " + (args["name"]?.string ?? "")
        case "tracker_mark_point": label = "Marked source pixel (\(args["x"]?.number ?? 0), \(args["y"]?.number ?? 0))"
        case "tracker_set_calibration": label = "Set scale reference \(args["length"]?.number ?? 0) \(args["unit"]?.string ?? "")"
        case "tracker_configure_tracking": label = "Changed tracking settings"
        case "tracker_start_tracking": label = "Started tracking"
        case "tracker_review_match": label = args["action"]?.string == "accept" ? "Accepted uncertain candidate" : "Skipped uncertain frame"
        case "tracker_cancel_job": label = "Stopped job " + String((args["job_id"]?.string ?? "").prefix(8))
        case "tracker_save_project": label = "Requested project save"
        case "tracker_export_csv": label = "Requested CSV export"
        case "tracker_undo": label = "Undid analysis edit"
        case "tracker_redo": label = "Redid analysis edit"
        default: label = command
        }
        return label + (sessionName.map { " · " + $0 } ?? "")
    }
    private func failure(_ error: Error) -> AutomationError {
        if let error = error as? AutomationError { return error }
        if error is CancellationError { return AutomationError("cancelled", "Operation cancelled.") }
        return AutomationError("operation_failed", error.localizedDescription)
    }
    private func session(_ args: [String:JSONValue], checkRevision: Bool = false) throws -> VideoSession {
        let id = try args.requiredID("session_id")
        guard let session = workspace.sessions.first(where: { $0.id == id }) else { throw AutomationError("session_not_found", "No open session with this ID.") }
        if checkRevision || args["expected_revision"] != nil {
            guard try args.requiredInt("expected_revision") == session.revision else { throw AutomationError("stale_revision", "Session changed. Inspect its current revision before editing or continuing pagination.") }
        }
        return session
    }
    private func path(_ args: [String:JSONValue], _ key: String = "path") throws -> URL {
        let value = try args.requiredString(key)
        guard value.hasPrefix("/"), !value.contains("\0") else { throw AutomationError("invalid_path", "\(key) must be an absolute local path.") }
        return URL(fileURLWithPath:value).standardizedFileURL
    }
    private func operation(_ kind: String, session: VideoSession? = nil, onCancel: (() -> Void)? = nil, body: @escaping @MainActor (AutomationOperation) async throws -> JSONValue) throws -> JSONValue {
        let operation = AutomationOperation(kind:kind,session:session); operation.cancelAction = onCancel; operations[operation.id] = operation
        operation.task = Task {
            defer { operation.task = nil; operation.cancelAction = nil }
            do {
                try Task.checkCancellation(); operation.state = "running"
                operation.result = try await body(operation)
                operation.state = "completed"
            } catch { operation.error = failure(error); operation.state = error is CancellationError ? "cancelled" : "failed" }
        }
        return try operation.snapshot()
    }
    private func tracking(_ session: VideoSession, id: UUID) throws -> JSONValue {
        let operation = AutomationOperation(id:id,kind:"tracking",session:session,trackingID:id)
        operations[id] = operation; return try operation.snapshot()
    }
    private func waitReady(_ session: VideoSession) async throws {
        let deadline = Date().addingTimeInterval(120)
        while session.loading || session.isSeeking {
            try Task.checkCancellation()
            guard !session.closed else { throw AutomationError("session_closed", "Session closed during operation.") }
            guard Date() < deadline else { throw AutomationError("operation_timeout", "Video indexing/frame read exceeded 120 seconds; inspect the native workspace.") }
            try await Task.sleep(for:.milliseconds(20))
        }
        try session.requireReady()
        guard session.displayedFrame != nil else { throw AutomationError("frame_unavailable", session.frameError ?? "No decoded frame is available.") }
    }
    private func mutate(_ name: String, _ a: [String:JSONValue]) throws -> JSONValue {
        switch name {
        case "tracker_import_video":
            let url = try path(a)
            guard FileManager.default.fileExists(atPath:url.path) else { throw AutomationError("file_not_found", url.path) }
            let created = !workspace.sessions.contains(where: { $0.url == url })
            let session = workspace.importVideo(url)
            return try operation("import",session:session,onCancel: {
                if created && session.loading {
                    session.close(); self.workspace.sessions.removeAll { $0.id == session.id }
                    if self.workspace.selectedID == session.id { self.workspace.selectedID = self.workspace.sessions.last?.id }
                }
            }) { _ in try await self.waitReady(session); return try self.describe(session) }
        case "tracker_open_project":
            let url = try path(a), relink = a["relink_path"] == nil ? nil : try path(a,"relink_path")
            return try operation("open_project") { op in
                let session = try await self.workspace.loadProject(url,relink:relink); op.session = session
                try await self.waitReady(session); return try self.describe(session)
            }
        case "tracker_cancel_job":
            let id = try a.requiredID("job_id")
            guard let op = operations[id] else { throw AutomationError("job_not_found", "No retained operation with this ID.") }
            if let trackingID = op.trackingID, let s = op.session {
                if s.lastTrackingID == trackingID, s.isTracking || s.review != nil { s.cancelTracking() }
            } else if ["queued","running"].contains(op.state) {
                // File writes commit atomically once begun; a completed write is reported as completed.
                op.cancelAction?(); op.task?.cancel()
            }
            return try op.snapshot()
        default: break
        }
        let s = try session(a,checkRevision:true)
        try s.requireReady()
        switch name {
        case "tracker_select_session": workspace.select(s.id)
        case "tracker_seek":
            let time = try a.requiredNumber("time")
            guard time >= 0, time <= s.duration else { throw AutomationError("invalid_time", "Time is outside this video.") }
            workspace.select(s.id); s.seek(time)
            let seekToken = s.seekToken
            return try operation("seek",session:s,onCancel: { if s.seekToken == seekToken { s.cancelSeek() } }) { _ in
                try await self.waitReady(s)
                guard s.seekToken == seekToken else { throw AutomationError("seek_superseded", "Another seek superseded this operation.") }
                return try self.describe(s)
            }
        case "tracker_add_track":
            let track = s.createTrack(name: a["name"]?.string ?? "Point mass \(s.tracks.count+1)")
            return .object(["track_id":.string(track.id.uuidString),"revision":.number(Double(s.revision))])
        case "tracker_mark_point":
            try s.recordPoint(CGPoint(x:try a.requiredNumber("x"),y:try a.requiredNumber("y")),trackID:try a.requiredID("track_id"),time:try a.requiredNumber("time"))
        case "tracker_set_calibration":
            try s.applyCalibration(Calibration(first:CGPoint(x:try a.requiredNumber("x1"),y:try a.requiredNumber("y1")),second:CGPoint(x:try a.requiredNumber("x2"),y:try a.requiredNumber("y2")),length:try a.requiredNumber("length"),unit:try a.requiredString("unit")))
        case "tracker_configure_tracking":
            try s.configure(TrackingSettings(templateRadius:try a.requiredInt("template_radius"),searchRadius:try a.requiredInt("search_radius"),minimumScore:try a.requiredNumber("minimum_score"),minimumSeparation:try a.requiredNumber("minimum_separation")))
        case "tracker_start_tracking": return try tracking(s,id:s.beginTracking(trackID:try a.requiredID("track_id")))
        case "tracker_review_match":
            let id = try a.requiredID("job_id"), action = try a.requiredString("action")
            guard s.lastTrackingID == id, s.review != nil else { throw AutomationError("review_not_found", "This job has no current uncertain candidate.") }
            guard !s.isSeeking else { throw AutomationError("frame_not_ready", "Wait for the pending source frame to load.") }
            if action == "accept" {
                guard s.review?.evidence.position != nil else { throw AutomationError("no_candidate", "No position to accept; mark a correction or skip.") }
                s.acceptCandidate()
            } else if action == "skip" { s.skipCandidate() }
            else { throw AutomationError("invalid_argument", "action must be accept or skip.") }
            guard let next = s.lastTrackingID, next != id else { throw AutomationError("continuation_failed", "Tracking could not continue.") }
            return try tracking(s,id:next)
        case "tracker_undo": s.undo()
        case "tracker_redo": s.redo()
        case "tracker_save_project", "tracker_export_csv":
            let destination = try path(a), overwrite = a["overwrite"]?.bool ?? false, revision = s.revision
            let trackID = name == "tracker_export_csv" ? try a.requiredID("track_id") : nil
            if let trackID { _ = try s.requireTrack(trackID) }
            return try operation(name == "tracker_save_project" ? "save" : "export",session:s) { _ in
                guard s.revision == revision else { throw AutomationError("stale_revision", "Session changed before the write began; inspect and retry with a new request ID.") }
                if let trackID { try await s.writeCSV(trackID:trackID,to:destination,overwrite:overwrite) }
                else { try await s.writeProject(to:destination,overwrite:overwrite) }
                return .object(["path":.string(destination.path),"saved_revision":.number(Double(revision))])
            }
        default: throw AutomationError("unknown_tool",name)
        }
        return try describe(s)
    }
    private func read(_ name: String, _ a: [String:JSONValue]) async throws -> JSONValue {
        if name == "tracker_list_sessions" {
            return .object(["sessions":.array(try workspace.sessions.map(describe)),"control_enabled":.bool(enabled),"selected_session_id":workspace.selectedID.map { .string($0.uuidString) } ?? .null])
        }
        if name == "tracker_get_job" {
            guard let op = operations[try a.requiredID("job_id")] else { throw AutomationError("job_not_found", "No retained operation with this ID in this app lifetime.") }
            return try op.snapshot()
        }
        let s = try session(a)
        switch name {
        case "tracker_get_session": return try describe(s)
        case "tracker_get_frame":
            try s.requireReady()
            let requested = a["time"]?.number ?? s.time
            guard requested >= 0, requested <= s.duration else { throw AutomationError("invalid_time", "Time is outside this video.") }
            let stamp = s.frameTimes.last(where: { $0.seconds <= requested+0.0000001 }) ?? s.frameTimes[0]
            let url = s.url, analysis = s.analysis, revision = s.revision
            let maxEdge = a["max_edge"] == nil ? 1280 : try a.requiredInt("max_edge")
            let crop = a["crop"]?.object
            let overlays = a["overlays"]?.bool ?? false
            return try await Task.detached(priority:.utility) {
                let reader = MediaSource(url:url)
                let frame = try await reader.image(at:stamp.seconds)
                return try FramePreview.render(frame,analysis:analysis,revision:revision,crop:crop,maxEdge:maxEdge,overlays:overlays)
            }.value
        case "tracker_get_review":
            guard let review = s.review, let jobID = s.lastTrackingID, let trackID = s.reviewTrackID else { return .object(["pending":.bool(false),"revision":.number(Double(s.revision))]) }
            return .object(["pending":.bool(true),"job_id":.string(jobID.uuidString),"track_id":.string(trackID.uuidString),"revision":.number(Double(s.revision)),"time":.number(review.frame.stamp.seconds),"frame":try .from(review.frame.stamp),"candidate":review.evidence.position.map { .object(["x":.number($0.x),"y":.number($0.y)]) } ?? .null,"score":.number(review.evidence.score),"separation":.number(review.evidence.separation),"reason":review.evidence.reason.map(JSONValue.string) ?? .null,"recorded":.bool(false),"score_is_probability":.bool(false)])
        case "tracker_get_measurements":
            let track = try s.requireTrack(a.requiredID("track_id"))
            let offset = a["offset"] == nil ? 0 : try a.requiredInt("offset"), limit = a["limit"] == nil ? 100 : try a.requiredInt("limit")
            guard (1...1000).contains(limit), offset <= track.points.count else { throw AutomationError("invalid_pagination", "limit must be 1...1000 and offset must not exceed the row count.") }
            let end = offset+min(limit,track.points.count-offset)
            let rows = try track.points[offset..<end].map { point -> JSONValue in
                let coordinate = s.coordinates(point.pixel)
                return .object(["id":.string(point.id.uuidString),"time":.number(point.time),"frame":try point.frame.map(JSONValue.from) ?? .null,"pixel_x":.number(point.pixel.x),"pixel_y":.number(point.pixel.y),"x":.number(coordinate.x),"y":.number(coordinate.y),"unit":.string(s.unit),"origin":.string(point.origin.rawValue),"match_score":point.score.map(JSONValue.number) ?? .null])
            }
            return .object(["revision":.number(Double(s.revision)),"total":.number(Double(track.points.count)),"offset":.number(Double(offset)),"next_offset":end < track.points.count ? .number(Double(end)) : .null,"measurements":.array(rows),"coordinate_origin":.string("bottom-left for x/y; top-left for pixel_x/pixel_y"),"score_is_probability":.bool(false)])
        default: throw AutomationError("unknown_tool",name)
        }
    }
    func describe(_ s: VideoSession) throws -> JSONValue {
        .object(["session_id":.string(s.id.uuidString),"revision":.number(Double(s.revision)),"source_path":.string(s.url.path),"source_width":.number(s.size.width),"source_height":.number(s.size.height),"duration":.number(s.duration),"time":.number(s.time),"frame":try s.displayedFrame.map { try .from($0.stamp) } ?? .null,"loading":.bool(s.loading),"seeking":.bool(s.isSeeking),"playing":.bool(s.playing),"closed":.bool(s.closed),"error":s.error.map(JSONValue.string) ?? .null,"calibration":try s.calibration.map(JSONValue.from) ?? .null,"settings":try .from(s.settings),"tracks":.array(s.tracks.map { .object(["track_id":.string($0.id.uuidString),"name":.string($0.name),"count":.number(Double($0.points.count))]) }),"job_id":s.lastTrackingID.map { .string($0.uuidString) } ?? .null,"job_state":.string(s.jobState),"dirty":.bool(s.isDirty)])
    }
}
