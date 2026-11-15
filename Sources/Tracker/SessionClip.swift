import Foundation
import CoreMedia
import TrackerAutomation

@MainActor
extension VideoSession {
    var clip: VideoClip? { analysis.clip ?? VideoClip.full(source:frameTimes) }
    var currentSourceFrame: Int { frameTimes.isEmpty ? 0 : MediaSource.frameIndex(at:time,in:frameTimes) }
    var usesSampledPlayback: Bool { (clip?.stepSize ?? 1) > 1 }
    var playbackRange: ClosedRange<Double> {
        let start = clip?.startStamp.seconds ?? 0
        return start...max(start+0.000001,clip?.lastSampleStamp.seconds ?? duration)
    }
    var clipSummary: String {
        guard let clip else { return "Indexing frames…" }
        return "Frames \(clip.startFrame)–\(clip.endFrame) · step \(clip.stepSize) · \(clip.sampleCount) samples"
    }
    func isSelected(_ stamp: FrameStamp) -> Bool {
        guard let clip, !frameTimes.isEmpty else { return false }
        let index = MediaSource.frameIndex(at:stamp.seconds,in:frameTimes)
        return clip.contains(index:index) && CMTimeCompare(frameTimes[index].cmTime,stamp.cmTime) == 0
    }
    func selectedObservations(_ points: [Measurement]) -> [Measurement] {
        guard let clip, !clip.isFullSource else { return points }
        return points.filter { point in
            if let stamp = point.frame { return selectedFrameSeconds.contains(stamp.seconds) }
            let index = MediaSource.frameIndex(at:point.time,in:frameTimes)
            return clip.contains(index:index) && abs(frameTimes[index].seconds-point.time) < 0.000001
        }
    }
    func setClip(start: Int, end: Int, step: Int) throws {
        try requireReady()
        let value = try VideoClip.make(start:start,end:end,step:step,source:frameTimes)
        let persisted: VideoClip? = value.isFullSource ? nil : value
        guard analysis.clip != persisted else { return }
        edit { $0.clip = persisted }
    }
    func resetClip() throws {
        try requireReady()
        guard !frameTimes.isEmpty else { throw ClipError.invalidBounds }
        try setClip(start:0,end:frameTimes.count-1,step:1)
    }
}
