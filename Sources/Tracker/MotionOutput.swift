import Foundation
import TrackerAutomation

enum MotionOutput {
    static func metadata(coordinates: CoordinateSystem, size: CGSize, settings: MotionSettings) throws -> JSONValue {
        let origin = coordinates.origin(in:size)
        return .object(["method":.string(settings.method),"window":.number(Double(settings.window)),
            "origin_x_px":.number(origin.x),"origin_y_px":.number(origin.y),"angle_degrees":.number(coordinates.angleDegrees),
            "gap_policy":.string("require_consecutive_source_frames"),"endpoints":.string("unavailable"),
            "positions_smoothed":.bool(false),"derivatives_smoothed":.bool(settings.window > 3)])
    }
    static func json(_ row: MotionRow, unit: String) throws -> JSONValue {
        let p = row.observation
        func optional(_ x: Double?) -> JSONValue { x.map(JSONValue.number) ?? .null }
        return .object(["id":.string(p.id.uuidString),"time":.number(row.sourceTime),"frame":try p.frame.map(JSONValue.from) ?? .null,
            "pixel_x":.number(p.pixel.x),"pixel_y":.number(p.pixel.y),"x":.number(row.x),"y":.number(row.y),
            "unit":.string(unit),"origin":.string(p.origin.rawValue),"match_score":optional(p.score),
            "vx":optional(row.vx),"vy":optional(row.vy),"speed":optional(row.speed),"ax":optional(row.ax),"ay":optional(row.ay),
            "velocity_unit":.string(unit+"/s"),"acceleration_unit":.string(unit+"/s²"),
            "derivative_unavailable_reason":row.unavailableReason.map(JSONValue.string) ?? .null])
    }
    static func csv(_ rows: [MotionRow], unit: String, coordinates: CoordinateSystem, size: CGSize, settings: MotionSettings) throws -> Data {
        var text = "time_s,x_\(unit),y_\(unit),source,match_score,vx_\(unit)_s,vy_\(unit)_s,speed_\(unit)_s,ax_\(unit)_s2,ay_\(unit)_s2,derivative_unavailable_reason,derivative_method,window_points,origin_x_px,origin_y_px,angle_degrees,frame_value,frame_timescale\n"
        let o = coordinates.origin(in:size)
        func number(_ value: Double?) -> String { value.map { String($0) } ?? "" }
        for (i,row) in rows.enumerated() {
            if i % 128 == 0 { try Task.checkCancellation() }
            let p = row.observation
            let values: [String] = [String(row.sourceTime),String(row.x),String(row.y),p.origin.rawValue,number(p.score),
                number(row.vx),number(row.vy),number(row.speed),number(row.ax),number(row.ay),row.unavailableReason ?? "",
                settings.method,String(settings.window),String(Double(o.x)),String(Double(o.y)),String(coordinates.angleDegrees),
                p.frame.map{String($0.value)} ?? "",p.frame.map{String($0.timescale)} ?? ""]
            text += values.joined(separator:",")+"\n"
        }
        return Data(text.utf8)
    }
}
