import Foundation
import Testing
@testable import Tracker

struct CSVExportTests {
    @Test @MainActor func profileTransitionsKeepDisplayedOptionsConsistent() {
        let preferences = CSVExportPreferences()
        preferences.decimalSeparator = .comma
        preferences.profile = .standard
        #expect(preferences.decimalSeparator == .dot)
        #expect(preferences.includeProvenance)
        #expect(preferences.options == CSVExportOptions())
        preferences.profile = .excel
        #expect(preferences.decimalSeparator == .recommended(for:.current))
        #expect(!preferences.includeProvenance)
    }
    private var rows: [MotionRow] {
        [MotionRow(observation:Tracker.Measurement(time:0.125125,pixel:CGPoint(x:20,y:30),frame:FrameStamp(value:3003,timescale:24000)),
            x:1.234567891234,y:-2.5,vx:3.25,vy:4,ax:nil,ay:nil,unavailableReason:"insufficient_neighbors")]
    }
    private func export(_ options: CSVExportOptions) throws -> Data {
        try MotionOutput.csv(rows,unit:"m",coordinates:CoordinateSystem(),size:CGSize(width:160,height:120),settings:MotionSettings(),options:options)
    }
    @Test func standardRetainsOriginalSchemaAndFullPrecision() throws {
        let data = try export(CSVExportOptions()), text = String(decoding:data,as:UTF8.self)
        #expect(!data.starts(with:[0xEF,0xBB,0xBF]))
        #expect(text.hasPrefix("time_s,x_m,y_m,source,match_score,vx_m_s"))
        #expect(text.contains("1.234567891234")); #expect(text.contains(",3003,24000,,,,\n"))
        #expect(!text.contains("sep="))
    }
    @Test(arguments:["id_ID","de_DE","en_US"]) func excelUsesExplicitRegionalSeparators(locale: String) throws {
        let options = CSVExportOptions.excel(locale:Locale(identifier:locale))
        let data = try export(options), text = String(decoding:data,as:UTF8.self)
        #expect(data.starts(with:[0xEF,0xBB,0xBF]))
        #expect(text.hasPrefix("\u{FEFF}sep=\(options.delimiter)\r\n"))
        #expect(text.components(separatedBy:"\r\n")[1].components(separatedBy:options.delimiter).count == 8)
        #expect(!text.contains("local-quadratic")); #expect(!text.contains("manual"))
        if locale == "en_US" { #expect(text.contains("0.125125,1.234567891234,-2.5")) }
        else { #expect(text.contains("0,125125;1,234567891234;-2,5")) }
        #expect(text.contains("m/s²"))
    }
    @Test func roundingIsExplicitAndCannotModifyMeasurements() throws {
        var options = CSVExportOptions.excel(locale:Locale(identifier:"id_ID")); options.decimalPlaces = 6
        let original = rows
        let text = String(decoding:try export(options),as:UTF8.self)
        #expect(text.contains("1,234568")); #expect(!text.contains("1,234567891234"))
        #expect(rows.first?.x == original.first?.x)
        options.decimalPlaces = 16; #expect(throws:(any Error).self) { try export(options) }
    }
    @Test func fieldsQuoteDelimitersQuotesAndNewlines() {
        let standard = CSVExportOptions()
        #expect(standard.field("a,b") == "\"a,b\"")
        #expect(standard.field("a\"b") == "\"a\"\"b\"")
        #expect(standard.field("a\nb") == "\"a\nb\"")
        let excel = CSVExportOptions.excel(locale:Locale(identifier:"id_ID"))
        #expect(excel.field("a;b") == "\"a;b\"")
        #expect(excel.field("1,25") == "1,25")
    }
    @Test func invalidCombinationsAndNonfiniteNumbersFailClearly() throws {
        #expect(throws:(any Error).self) { try export(CSVExportOptions(decimalSeparator:.comma)) }
        #expect(throws:(any Error).self) { try CSVExportOptions().number(.infinity) }
        #expect(try CSVExportOptions().number(nil) == "")
        let options = CSVExportOptions(profile:.excel,decimalSeparator:.comma,includeProvenance:true)
        let text = String(decoding:try export(options),as:UTF8.self)
        #expect(text.contains("insufficient_neighbors")); #expect(text.contains("three-point-quadratic-v1"))
    }
}
