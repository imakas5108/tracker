import Foundation
import Combine

@MainActor
final class MotionPresentation: ObservableObject {
    @Published private(set) var rows: [MotionRow] = []
    @Published private(set) var plotRows: [MotionRow] = []
    @Published var updating = false
    @Published var error: String?
    private(set) var revision = -1
    func clear() {
        rows = []; plotRows = []; error = nil; updating = false; revision = -1
    }
    func set(_ rows: [MotionRow], revision: Int) {
        self.rows = rows; self.revision = revision
        // Display reduction affects markers only. Tables, exports and calculations keep every row.
        let stride = max(1, Int(ceil(Double(rows.count)/2000)))
        plotRows = Swift.stride(from:0,to:rows.count,by:stride).map { rows[$0] }
        if let last = rows.last, plotRows.last?.id != last.id { plotRows.append(last) }
        updating = false; error = nil
    }
}
