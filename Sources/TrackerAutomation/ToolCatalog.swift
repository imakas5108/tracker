import Foundation

public struct ToolDefinition: Sendable {
    public let name: String
    public let description: String
    public let schema: JSONValue
    public let readOnly: Bool
}

public enum ToolCatalog {
    public static let tools: [ToolDefinition] = {
        let string: JSONValue = .object(["type": .string("string")])
        let number: JSONValue = .object(["type": .string("number")])
        let integer: JSONValue = .object(["type": .string("integer"), "minimum": .number(0)])
        let boolean: JSONValue = .object(["type": .string("boolean")])
        var result: [ToolDefinition] = []
        func add(_ name: String, _ description: String, _ properties: [String: JSONValue] = [:], _ required: [String] = [], mutation: Bool = false, session: Bool = false) {
            var props = properties, needs = required
            if session { props["session_id"] = string; needs.append("session_id") }
            if mutation {
                props["request_id"] = .object(["type": .string("string"), "description": .string("Unique UUID for this mutation. Reuse unchanged to retry; retained for this app lifetime.")]); needs.append("request_id")
                if session { props["expected_revision"] = integer; needs.append("expected_revision") }
            }
            result.append(ToolDefinition(name: "tracker_"+name, description: description,
                schema: .object(["type": .string("object"), "properties": .object(props), "required": .array(needs.map(JSONValue.string)), "additionalProperties": .bool(false)]), readOnly: !mutation))
        }
        add("list_sessions", "List open native video sessions, stable IDs, revisions and control state.")
        add("import_video", "Open an absolute local video path in Tracker. Returns a retained operation ID; poll get_job.", ["path": string], ["path"], mutation: true)
        add("open_project", "Open a saved Tracker JSON project. Missing footage returns relink_required; supply relink_path explicitly.", ["path": string, "relink_path": string], ["path"], mutation: true)
        add("get_session", "Inspect source, tracks, calibration, current frame, revision and tracking settings.", session: true)
        add("select_session", "Select this explicit native workspace tab.", mutation: true, session: true)
        add("seek", "Pause and seek the native viewer to the source frame at or preceding time in seconds. Returns operation ID.", ["time": number], ["time"], mutation: true, session: true)
        let crop: JSONValue = .object(["type": .string("object"), "properties": .object(["x": integer,"y": integer,"width": integer,"height": integer]), "required": .array(["x","y","width","height"].map(JSONValue.string)), "additionalProperties": .bool(false)])
        add("get_frame", "Return PNG and actual source timestamp without seeking or interrupting playback. Coordinates are orientation-corrected source pixels, top-left origin; crop precedes resize. Default max_edge 1280, maximum 1600. Overlays show observed marks and calibration only.", ["time": number,"crop": crop,"max_edge": integer,"overlays": boolean], session: true)
        add("add_track", "Add and select an undoable point track.", ["name": string], mutation: true, session: true)
        add("mark_point", "Record/correct an observed position on the paused displayed frame. Supply its exact source time and explicit track ID; x/y are original top-left source pixels.", ["track_id":string,"time":number,"x":number,"y":number], ["track_id","time","x","y"], mutation: true, session: true)
        add("set_calibration", "Set an explicit scale reference. Endpoints use source pixels; length is the supplied known physical distance. Supported units: m, cm, mm. No scale is inferred.", ["x1":number,"y1":number,"x2":number,"y2":number,"length":number,"unit":string], ["x1","y1","x2","y2","length","unit"], mutation:true,session:true)
        add("set_axes", "Set static coordinate axes without restarting tracking. Origin is in orientation-corrected top-left source pixels; angle_degrees is counterclockwise from horizontal, -360...360. Undoable.", ["origin_x":number,"origin_y":number,"angle_degrees":number],["origin_x","origin_y","angle_degrees"],mutation:true,session:true)
        add("configure_motion", "Set derivative window: 3 (unsmoothed three-point quadratic), or 5/7/9 (local quadratic smoothing). Uses actual timestamps; endpoints and windows crossing missing source observations return null with a reason. Raw positions are unchanged. Undoable; does not restart tracking.", ["window":integer],["window"],mutation:true,session:true)
        add("configure_tracking", "Set fixed-template settings. Match scores and competing-candidate separation are heuristic scores, never probabilities.", ["template_radius":integer,"search_radius":integer,"minimum_score":number,"minimum_separation":number], ["template_radius","search_radius","minimum_score","minimum_separation"],mutation:true,session:true)
        add("start_tracking", "Start an app-owned tracking job from this track's observation on the displayed frame. Uses the existing shared execution slot. Poll get_job; uncertainty requires explicit review.", ["track_id":string],["track_id"],mutation:true,session:true)
        add("get_job", "Inspect a retained import, seek, write or tracking operation by job_id, including terminal/review state.", ["job_id":string],["job_id"])
        add("get_review", "Inspect the unrecorded candidate, heuristic score, separation, reason, source timestamp and owning job/track IDs. Retrieve its image with get_frame.",session:true)
        add("review_match", "Explicitly accept or skip the pending candidate for this job. Acceptance records a reviewed observation; skip records none. Returns a continuation job ID.", ["job_id":string,"action": .object(["type":.string("string"),"enum":.array([.string("accept"),.string("skip")])])],["job_id","action"],mutation:true,session:true)
        add("cancel_job", "Cancel an app-owned job, including queued tracking. Terminal records remain inspectable.", ["job_id":string],["job_id"],mutation:true)
        add("get_measurements", "Read paginated observations and derived vx/vy/speed/ax/ay using configured axes, with units, method, exact frame stamps and missing-value reasons. Supply expected_revision across pages to reject changing data.", ["track_id":string,"offset":integer,"limit":integer,"expected_revision":integer],["track_id"],session:true)
        add("save_project", "Save a project to an absolute path asynchronously. Existing destinations require overwrite=true. Returns operation ID.", ["path":string,"overwrite":boolean],["path"],mutation:true,session:true)
        add("export_csv", "Export an explicit track asynchronously to an absolute CSV path. Existing destinations require overwrite=true. Returns operation ID.", ["track_id":string,"path":string,"overwrite":boolean],["track_id","path"],mutation:true,session:true)
        add("undo", "Undo the latest scientific edit/run in this session, cancelling its current tracking job.",mutation:true,session:true)
        add("redo", "Redo a scientific edit in this session.",mutation:true,session:true)
        return result
    }()

    public static func validate(_ request: AutomationRequest) throws -> ToolDefinition {
        guard let tool = tools.first(where: { $0.name == request.command }) else { throw AutomationError("unknown_tool", request.command) }
        let properties = tool.schema["properties"].object ?? [:]
        guard request.arguments.keys.allSatisfy({ properties[$0] != nil }) else { throw AutomationError("invalid_argument", "Unknown argument.") }
        for key in tool.schema["required"].array?.compactMap(\.string) ?? [] {
            guard request.arguments[key] != nil else { throw AutomationError("invalid_argument", "Missing \(key).") }
        }
        for (key, value) in request.arguments {
            let kind = properties[key]?["type"].string
            let valid: Bool
            switch kind {
            case "string": valid = value.string != nil
            case "number": valid = value.number?.isFinite == true
            case "integer": valid = value.number.map { $0.isFinite && $0 >= 0 && $0.rounded() == $0 && $0 < Double(Int.max) } ?? false
            case "boolean": valid = value.bool != nil
            case "object": valid = value.object != nil
            default: valid = false
            }
            guard valid else { throw AutomationError("invalid_argument", "Invalid type or range for \(key).") }
        }
        return tool
    }
}
