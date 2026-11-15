import Foundation
import CoreMedia

struct VideoClip: Codable, Equatable, Sendable {
    var startFrame: Int
    var endFrame: Int
    var stepSize: Int
    var sourceFrameCount: Int
    var startStamp: FrameStamp
    var endStamp: FrameStamp
    var lastSampleStamp: FrameStamp

    private var validBounds: Bool {
        sourceFrameCount > 0 && startFrame >= 0 && endFrame >= startFrame &&
        endFrame < sourceFrameCount && stepSize > 0
    }
    var sampleCount: Int { validBounds ? (endFrame-startFrame)/stepSize+1 : 0 }
    var lastSampleFrame: Int { sampleCount > 0 ? startFrame+(sampleCount-1)*stepSize : startFrame }
    var isFullSource: Bool { validBounds && startFrame == 0 && endFrame == sourceFrameCount-1 && stepSize == 1 }
    func contains(index: Int) -> Bool {
        validBounds && index >= startFrame && index <= endFrame && (index-startFrame)%stepSize == 0
    }
    func frameAtOrBefore(_ index: Int) -> Int {
        guard validBounds else { return 0 }
        if index <= startFrame { return startFrame }
        if index >= lastSampleFrame { return lastSampleFrame }
        return startFrame+(index-startFrame)/stepSize*stepSize
    }
    func advanced(from index: Int, steps: Int) -> Int {
        guard sampleCount > 0 else { return 0 }
        let current = (frameAtOrBefore(index)-startFrame)/stepSize
        let (proposed, overflow) = current.addingReportingOverflow(steps)
        let next = overflow ? (steps > 0 ? sampleCount-1 : 0) : min(sampleCount-1,max(0,proposed))
        return startFrame+next*stepSize
    }
    func validated() throws -> Self {
        guard validBounds,
              [startStamp,endStamp,lastSampleStamp].allSatisfy({ $0.timescale > 0 && $0.value >= 0 }),
              CMTimeCompare(startStamp.cmTime,lastSampleStamp.cmTime) <= 0,
              CMTimeCompare(lastSampleStamp.cmTime,endStamp.cmTime) <= 0,
              (sampleCount > 1 ? CMTimeCompare(startStamp.cmTime,lastSampleStamp.cmTime) < 0 : CMTimeCompare(startStamp.cmTime,lastSampleStamp.cmTime) == 0),
              (endFrame > startFrame ? CMTimeCompare(startStamp.cmTime,endStamp.cmTime) < 0 : CMTimeCompare(startStamp.cmTime,endStamp.cmTime) == 0)
        else { throw ClipError.invalidBounds }
        return self
    }
    func validate(source: [FrameStamp]) throws {
        _ = try validated()
        guard source.count == sourceFrameCount,
              CMTimeCompare(source[startFrame].cmTime,startStamp.cmTime) == 0,
              CMTimeCompare(source[endFrame].cmTime,endStamp.cmTime) == 0,
              CMTimeCompare(source[lastSampleFrame].cmTime,lastSampleStamp.cmTime) == 0
        else { throw ClipError.sourceMismatch }
    }
    func timestamps(in source: [FrameStamp]) throws -> [FrameStamp] {
        try validate(source:source)
        if isFullSource { return source }
        return (0..<sampleCount).map { source[startFrame+$0*stepSize] }
    }
    static func make(start: Int, end: Int, step: Int, source: [FrameStamp]) throws -> Self {
        guard !source.isEmpty, start >= 0, end >= start, end < source.count, step > 0 else { throw ClipError.invalidBounds }
        let last = start+(end-start)/step*step
        return try Self(startFrame:start,endFrame:end,stepSize:step,sourceFrameCount:source.count,
            startStamp:source[start],endStamp:source[end],lastSampleStamp:source[last]).validated()
    }
    static func full(source: [FrameStamp]) -> Self? {
        try? make(start:0,end:source.count-1,step:1,source:source)
    }
}

enum ClipError: LocalizedError, Equatable {
    case invalidBounds, sourceMismatch
    var errorDescription: String? {
        switch self {
        case .invalidBounds: "Choose source-frame bounds with 0 ≤ start ≤ end < frame count, and a positive step size."
        case .sourceMismatch: "The saved clip does not match this source frame count or boundary timestamps. Relink the original footage."
        }
    }
}
