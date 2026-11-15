import Foundation
import Testing
@testable import Tracker

struct ClipTests {
    private var source: [FrameStamp] {
        [0,1,3,4,6,7,9,10,12,13,15,16].map { FrameStamp(value:Int64($0)*1001,timescale:24000) }
    }
    @Test func boundsAreInclusiveAndSamplingIsAnchoredToStart() throws {
        let clip = try VideoClip.make(start:2,end:9,step:3,source:source)
        #expect(clip.sampleCount == 3)
        #expect(clip.lastSampleFrame == 8)
        #expect(try clip.timestamps(in:source) == [source[2],source[5],source[8]])
        #expect(!clip.contains(index:9))
        #expect(clip.frameAtOrBefore(Int.min) == 2)
        #expect(clip.frameAtOrBefore(7) == 5)
        #expect(clip.frameAtOrBefore(Int.max) == 8)
        #expect(clip.advanced(from:5,steps:Int.max) == 8)
        #expect(clip.advanced(from:5,steps:Int.min) == 2)
        #expect(clip.startStamp == source[2])
        #expect(clip.endStamp == source[9])
    }
    @Test func singleSampleAndExtremeInputsCannotOverflow() throws {
        let one = try VideoClip.make(start:4,end:11,step:Int.max,source:source)
        #expect(one.sampleCount == 1)
        #expect(try one.timestamps(in:source) == [source[4]])
        #expect(one.advanced(from:4,steps:Int.max) == 4)
        #expect(VideoClip.full(source:[]) == nil)
        for (start,end,step) in [(-1,3,1),(0,12,1),(4,3,1),(0,4,0),(0,4,Int.min),(Int.max,Int.max,1)] {
            #expect(throws:ClipError.invalidBounds) { try VideoClip.make(start:start,end:end,step:step,source:source) }
        }
    }
    @Test func relinkChecksCountsAndRationalBoundaryTimes() throws {
        let clip = try VideoClip.make(start:2,end:9,step:3,source:source)
        var different = source
        different[8].value += 1
        #expect(throws:ClipError.sourceMismatch) { try clip.validate(source:different) }
        #expect(throws:ClipError.sourceMismatch) { try clip.validate(source:Array(source.dropLast())) }
        let equivalent = source.map { FrameStamp(value:$0.value*2,timescale:$0.timescale*2) }
        try clip.validate(source:equivalent)
        var invalid = clip; invalid.stepSize = 0
        #expect(throws:ClipError.invalidBounds) { try invalid.validated() }
    }
    @Test func persistedSelectionRequiresVersionFourAndRetainsAllPoints() throws {
        let all = source.map { Measurement(time:$0.seconds,pixel:CGPoint(x:20,y:30),frame:$0) }
        var state = AnalysisState(tracks:[PointTrack(name:"Retained",points:all)])
        var file = ProjectFile(source:URL(fileURLWithPath:"/tmp/clip.mov"),sourceSize:CGSize(width:160,height:120),analysis:state,time:0,settings:TrackingSettings())
        for version in 1...3 {
            file.version = version
            let legacy = try JSONDecoder().decode(ProjectFile.self,from:JSONEncoder().encode(file)).validated()
            #expect(legacy.analysis.clip == nil)
        }
        state.clip = try VideoClip.make(start:2,end:9,step:3,source:source)
        file.analysis = state
        #expect(throws:(any Error).self) { try file.validated() }
        file.version = 4
        let restored = try JSONDecoder().decode(ProjectFile.self,from:JSONEncoder().encode(file)).validated()
        #expect(restored.analysis.clip == state.clip)
        #expect(restored.analysis.tracks[0].points == all)
    }
    @Test func sampledDerivativesUseSourceTimeAndKeepRealGapsUnavailable() throws {
        let clip = try VideoClip.make(start:1,end:11,step:2,source:source)
        let selected = try clip.timestamps(in:source)
        let points = selected.map { stamp in
            Measurement(time:stamp.seconds,pixel:CGPoint(x:20+3*stamp.seconds+2*stamp.seconds*stamp.seconds,y:60),frame:stamp)
        }
        func rows(_ points: [Tracker.Measurement]) throws -> [MotionRow] {
            try MotionAnalysis.calculate(points:points,sourceTimes:selected,size:CGSize(width:160,height:120),calibration:nil,coordinates:CoordinateSystem(),settings:MotionSettings())
        }
        let complete = try rows(points)
        #expect(abs(try #require(complete[2].vx)-(3+4*selected[2].seconds)) < 1e-9)
        #expect(abs(try #require(complete[2].ax)-4) < 1e-8)
        let missing = try rows(points.enumerated().filter { $0.offset != 3 }.map(\.element))
        #expect(missing[2].vx == nil)
        #expect(missing[2].unavailableReason == "gap_in_observations")
        let data = try MotionOutput.csv(complete,unit:"px",coordinates:CoordinateSystem(),size:CGSize(width:160,height:120),settings:MotionSettings(),clip:clip)
        let lines = String(decoding:data,as:UTF8.self).split(separator:"\n")
        #expect(lines[0].split(separator:",").count == 22)
        #expect(lines[1].hasSuffix(",1,11,2"))
    }
}
