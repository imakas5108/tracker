import Foundation
import TrackerAutomation

enum MotionOutput {
    static func metadata(coordinates: CoordinateSystem, size: CGSize, settings: MotionSettings, clip: VideoClip? = nil) throws -> JSONValue {
        let origin = coordinates.origin(in:size)
        return .object(["method":.string(settings.method),"window":.number(Double(settings.window)),
            "origin_x_px":.number(origin.x),"origin_y_px":.number(origin.y),"angle_degrees":.number(coordinates.angleDegrees),
            "gap_policy":.string((clip?.stepSize ?? 1) > 1 ? "require_consecutive_selected_steps" : "require_consecutive_source_frames"),"endpoints":.string("unavailable"),
            "clip_step_size":.number(Double(clip?.stepSize ?? 1)),
            "positions_smoothed":.bool(false),"derivatives_smoothed":.bool(settings.window > 3)])
    }
    static func json(_ row: MotionRow, unit: String) throws -> JSONValue {
        let p = row.observation
        func optional(_ x: Double?) -> JSONValue { x.map(JSONValue.number) ?? .null }
        return .object(["id":.string(p.id.uuidString),"time":.number(row.sourceTime),"frame":try p.frame.map(JSONValue.from) ?? .null,
            "pixel_x":.number(p.pixel.x),"pixel_y":.number(p.pixel.y),"x":.number(row.x),"y":.number(row.y),
            "unit":.string(unit),"origin":.string(p.origin.rawValue),"match_score":optional(p.score),
            "matching_method":p.matchingMethod.map { .string($0.rawValue) } ?? .null,
            "vx":optional(row.vx),"vy":optional(row.vy),"speed":optional(row.speed),"ax":optional(row.ax),"ay":optional(row.ay),
            "velocity_unit":.string(unit+"/s"),"acceleration_unit":.string(unit+"/s²"),
            "derivative_unavailable_reason":row.unavailableReason.map(JSONValue.string) ?? .null])
    }
    static func csv(_ rows: [MotionRow], unit: String, coordinates: CoordinateSystem, size: CGSize, settings: MotionSettings, options: CSVExportOptions = CSVExportOptions(), clip: VideoClip? = nil) throws -> Data {
        let options = try options.validated()
        let measurements = ["time_s","x_\(unit)","y_\(unit)","vx_\(unit)_s","vy_\(unit)_s","speed_\(unit)_s","ax_\(unit)_s2","ay_\(unit)_s2"]
        let details = ["source","match_score","derivative_unavailable_reason","derivative_method","window_points","origin_x_px","origin_y_px","angle_degrees","frame_value","frame_timescale","tracking_method","clip_start_frame","clip_end_frame","clip_step_size"]
        let headers: [String]
        if options.profile == .excel {
            headers = ["t (s)","x (\(unit))","y (\(unit))","vx (\(unit)/s)","vy (\(unit)/s)","v (\(unit)/s)","ax (\(unit)/s²)","ay (\(unit)/s²)"] + (options.includeProvenance ? details : [])
        } else if options.includeProvenance {
            headers = Array(measurements.prefix(3))+Array(details.prefix(2))+Array(measurements.dropFirst(3))+Array(details.dropFirst(2))
        } else { headers = measurements }
        let newline = options.profile == .excel ? "\r\n" : "\n"
        func line(_ values: [String]) -> String { values.map(options.field).joined(separator:options.delimiter)+newline }
        var text = options.profile == .excel ? "\u{FEFF}sep=\(options.delimiter)\r\n" : ""
        text += line(headers)
        let o = coordinates.origin(in:size)
        for (i,row) in rows.enumerated() {
            if i % 128 == 0 { try Task.checkCancellation() }
            let p = row.observation
            let values = try [row.sourceTime,row.x,row.y,row.vx,row.vy,row.speed,row.ax,row.ay].map(options.number)
            if options.includeProvenance {
                let provenance = try [p.origin.rawValue,options.number(p.score),row.unavailableReason ?? "",settings.method,String(settings.window),
                    options.number(Double(o.x)),options.number(Double(o.y)),options.number(coordinates.angleDegrees),
                    p.frame.map{String($0.value)} ?? "",p.frame.map{String($0.timescale)} ?? "",p.matchingMethod?.rawValue ?? "",
                    clip.map{String($0.startFrame)} ?? "",clip.map{String($0.endFrame)} ?? "",clip.map{String($0.stepSize)} ?? ""]
                if options.profile == .standard {
                    text += line(Array(values.prefix(3))+Array(provenance.prefix(2))+Array(values.dropFirst(3))+Array(provenance.dropFirst(2)))
                } else { text += line(values+provenance) }
            } else { text += line(values) }
        }
        return Data(text.utf8)
    }
}
