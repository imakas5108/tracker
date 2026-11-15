import Foundation
import TrackerAutomation

enum ClipOutput {
    static func json(_ clip: VideoClip?) throws -> JSONValue {
        guard let clip else { return .null }
        return .object(["start_frame":.number(Double(clip.startFrame)),"end_frame":.number(Double(clip.endFrame)),
            "last_sample_frame":.number(Double(clip.lastSampleFrame)),"step_size":.number(Double(clip.stepSize)),
            "sample_count":.number(Double(clip.sampleCount)),"source_frame_count":.number(Double(clip.sourceFrameCount)),
            "start_time":.number(clip.startStamp.seconds),"end_time":.number(clip.endStamp.seconds),
            "last_sample_time":.number(clip.lastSampleStamp.seconds),
            "start_stamp":try .from(clip.startStamp),"end_stamp":try .from(clip.endStamp),
            "last_sample_stamp":try .from(clip.lastSampleStamp),"full_source":.bool(clip.isFullSource)])
    }
}
