import Foundation
import Testing
@testable import Tracker

struct NormalizedMatcherTests {
    private func scene(_ centers: [CGPoint], gain: Double = 1, offset: Double = 0) -> GrayImage {
        var pixels = [UInt8](repeating:30, count:80*60)
        for center in centers {
            for y in -1...1 { for x in -1...1 {
                pixels[(Int(center.y)+y)*80+Int(center.x)+x] = UInt8(180+(x+1)*10+(y+1)*5)
            } }
        }
        return GrayImage(width:80,height:60,pixels:pixels.map { UInt8((Double($0)*gain+offset).rounded()) })
    }
    private var settings: TrackingSettings {
        TrackingSettings(templateRadius:12,searchRadius:10,minimumScore:0.9,minimumSeparation:0.015,matchingMethod:.normalizedCorrelation)
    }
    @Test func smallTargetIsNotDilutedByTemplateBackground() throws {
        let seed = CGPoint(x:35,y:30), target = CGPoint(x:42,y:32)
        let old = try TemplateMatcher(image:scene([seed]),center:seed,settings:TrackingSettings(templateRadius:12,searchRadius:10))
        #expect(!(try old.match(scene([target]),around:seed)).accepted)
        let matcher = try TemplateMatcher(image:scene([seed]),center:seed,settings:settings)
        let result = try matcher.match(scene([target]),around:seed)
        #expect(result.accepted)
        let position = try #require(result.position)
        #expect(abs(position.x-target.x) < 0.1)
        #expect(abs(position.y-target.y) < 0.1)
    }
    @Test func normalizationHandlesBrightnessAndContrastButRejectsAbsence() throws {
        let seed = CGPoint(x:35,y:30), target = CGPoint(x:39,y:33)
        let matcher = try TemplateMatcher(image:scene([seed]),center:seed,settings:settings)
        let changed = try matcher.match(scene([target],gain:0.8,offset:50),around:seed)
        #expect(changed.accepted)
        #expect(changed.score > 0.999)
        #expect(!(try matcher.match(scene([]),around:seed)).accepted)
    }
    @Test func competingIdenticalTargetsRemainUnrecordable() throws {
        let seed = CGPoint(x:35,y:30)
        let matcher = try TemplateMatcher(image:scene([seed]),center:seed,settings:settings)
        let result = try matcher.match(scene([CGPoint(x:27,y:30),CGPoint(x:43,y:30)]),around:seed)
        #expect(!result.accepted)
        #expect(result.separation < 0.000001)
    }
    @Test func legacyDefaultsAndProjectVersionPreventSilentMethodChanges() throws {
        let legacy = Data(#"{"templateRadius":12,"searchRadius":24,"minimumScore":0.9,"minimumSeparation":0.015}"#.utf8)
        #expect(try JSONDecoder().decode(TrackingSettings.self,from:legacy).matchingMethod == .squaredDifference)
        var file = ProjectFile(version:3,source:URL(fileURLWithPath:"/tmp/sample.mp4"),sourceSize:CGSize(width:80,height:60),analysis:AnalysisState(),time:0,settings:settings,algorithm:settings.matchingMethod.algorithm)
        let restored = try JSONDecoder().decode(ProjectFile.self,from:JSONEncoder().encode(file)).validated()
        #expect(restored.settings.matchingMethod == .normalizedCorrelation)
        file.version = 2
        #expect(throws:(any Error).self) { try file.validated() }
        file.version = 3; file.algorithm = "fixed-grayscale-ssd-v1"
        #expect(throws:(any Error).self) { try file.validated() }
    }
    @Test @MainActor func changingSettingsCannotRelabelEarlierObservations() throws {
        let session = VideoSession(url:URL(fileURLWithPath:"/tmp/sample.mp4"))
        let observation = Tracker.Measurement(time:0,pixel:CGPoint(x:25,y:25),origin:.automatic,score:0.95,matchingMethod:.normalizedCorrelation)
        let track = PointTrack(name:"Mixed methods",points:[observation])
        session.size = CGSize(width:80,height:60)
        session.edit { $0 = AnalysisState(tracks:[track],selectedTrackID:track.id) }
        let snapshot = try session.snapshot().validated()
        #expect(snapshot.version == 3)
        #expect(snapshot.settings.matchingMethod == .squaredDifference)
        let roundTrip = try JSONDecoder().decode(ProjectFile.self,from:JSONEncoder().encode(snapshot))
        #expect(roundTrip.analysis.tracks[0].points[0].matchingMethod == .normalizedCorrelation)
        let value = try MotionOutput.json(MotionRow(observation:observation,x:25,y:35),unit:"px")
        #expect(value["matching_method"].string == "zncc")
        session.close()
    }
}
