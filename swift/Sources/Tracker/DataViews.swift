import SwiftUI
import Charts

struct PositionPlot: View {
    @ObservedObject var session: VideoSession
    var body: some View { MotionPlot(session:session,data:session.motionData) }
}

private struct MotionPlot: View {
    @ObservedObject var session: VideoSession
    @ObservedObject var data: MotionPresentation
    @State private var horizontal = MotionVariable.time
    @State private var vertical = MotionVariable.x
    var body: some View {
        VStack(spacing:0) {
            HStack {
                Label("Plot",systemImage:"chart.xyaxis.line").font(.callout.weight(.semibold))
                ProgressView().controlSize(.mini).opacity(data.updating ? 1 : 0)
                    .accessibilityLabel("Updating motion results").accessibilityHidden(!data.updating)
                Spacer()
                DerivativeMenu(session:session)
            }.padding(.horizontal,12).frame(height:36)
            Divider()
            HStack {
                variablePicker("Vertical axis",selection:$vertical)
                Spacer()
            }.padding(.horizontal,10).padding(.top,6)
            if let error = data.error {
                Text(error).font(.caption).foregroundStyle(.secondary).padding()
            } else if data.rows.isEmpty {
                Text(data.updating ? "Calculating motion…" : "Mark positions to plot motion over time.")
                    .font(.callout).foregroundStyle(.secondary).padding().frame(maxHeight:.infinity)
            } else if !data.plotRows.contains(where: { horizontal.value($0) != nil && vertical.value($0) != nil }) {
                Text("No paired values for these axes. Derivatives need consecutive source-frame observations.")
                    .font(.callout).foregroundStyle(.secondary).padding().frame(maxHeight:.infinity)
            } else {
                Chart(data.plotRows) { row in
                    if let x = horizontal.value(row), let y = vertical.value(row) {
                        PointMark(x:.value(horizontal.rawValue,x),y:.value(vertical.rawValue,y))
                            .foregroundStyle(Color.accentColor)
                            .symbol(by:.value("Source",row.observation.origin.rawValue))
                            .symbolSize(abs(row.sourceTime-session.time)<0.000001 ? 65 : 16)
                    }
                }.padding(.horizontal,12).padding(.top,8)
                    .accessibilityLabel("\(vertical.title) versus \(horizontal.title)")
            }
            HStack {
                Spacer()
                variablePicker("Horizontal axis",selection:$horizontal)
            }.padding(.horizontal,10).padding(.vertical,4)
            if data.rows.count > data.plotRows.count {
                Text("\(data.plotRows.count) plot markers · all \(data.rows.count) rows retained")
                    .font(.caption2).foregroundStyle(.secondary).padding(.bottom,4)
            }
        }.frame(maxWidth:.infinity,maxHeight:.infinity)
    }
    private func variablePicker(_ title: String, selection: Binding<MotionVariable>) -> some View {
        Menu {
            Picker(title,selection:selection) {
                ForEach(MotionVariable.allCases) { variable in
                    Text("\(variable.title) (\(variable.unit(session.unit)))").tag(variable)
                }
            }
        } label: {
            Text("\(selection.wrappedValue.rawValue) (\(selection.wrappedValue.unit(session.unit)))")
        }.menuStyle(.borderlessButton).fixedSize().accessibilityLabel(title)

    }
}

private struct DerivativeMenu: View {
    @ObservedObject var session: VideoSession
    var body: some View {
        Menu {
            ForEach([3,5,7,9],id:\.self) { window in
                Button {
                    do { try session.setMotionSettings(MotionSettings(window:window)) }
                    catch { session.exportError = error.localizedDescription }
                } label: {
                    if session.motionSettings.window == window {
                        Label(window == 3 ? "3-point · no smoothing" : "\(window)-point local quadratic",systemImage:"checkmark")
                    } else { Text(window == 3 ? "3-point · no smoothing" : "\(window)-point local quadratic") }
                }
            }
            Divider()
            Text("Endpoints and gaps have no derivatives.")
            Text("Wider windows smooth derivatives, not positions.")
        } label: { Text("\(session.motionSettings.window)-point") }
        .menuStyle(.borderlessButton).fixedSize().help("Derivative method: \(session.motionSettings.description). Differentiation amplifies noise.")
        .accessibilityLabel("Derivative method")
    }
}

struct DataPanel: View {
    @ObservedObject var session: VideoSession
    var body: some View { MotionTable(session:session,data:session.motionData) }
}

private struct MotionTable: View {
    @ObservedObject var session: VideoSession
    @ObservedObject var data: MotionPresentation
    @State private var columns: Set<MotionVariable> = [.x,.y]
    var body: some View {
        VStack(spacing:0) {
            HStack {
                Label("Table",systemImage:"tablecells").font(.callout.weight(.semibold))
                ProgressView().controlSize(.mini).opacity(data.updating ? 1 : 0)
                    .accessibilityLabel("Updating motion results").accessibilityHidden(!data.updating)
                Spacer()
                Menu("Columns") {
                    ForEach(MotionVariable.allCases.filter { $0 != .time }) { variable in
                        Toggle(variable.title,isOn:Binding(get:{columns.contains(variable)},set:{ selected in
                            if selected { columns.insert(variable) } else { columns.remove(variable) }
                        }))
                    }
                }.menuStyle(.borderlessButton).fixedSize().accessibilityLabel("Table columns")
                    .disabled(!supportsColumns)
            }.padding(.horizontal,12).frame(height:36)
            Divider()
            if data.rows.isEmpty {
                Text(data.error ?? (data.updating ? "Calculating motion…" : "No measurements yet"))
                    .font(.callout).foregroundStyle(.secondary).padding().frame(maxWidth:.infinity,maxHeight:.infinity)
            } else {
                if #available(macOS 14.4, *) {
                Table(data.rows) {
                    TableColumn("t (s)") { row in cell(row,variable:.time) }.width(min:64,ideal:72)
                    if columns.contains(.x) { TableColumn("x (\(session.unit))") { row in cell(row,variable:.x) }.width(min:64,ideal:80) }
                    if columns.contains(.y) { TableColumn("y (\(session.unit))") { row in cell(row,variable:.y) }.width(min:64,ideal:80) }
                    if columns.contains(.vx) { TableColumn("vx (\(session.unit)/s)") { row in cell(row,variable:.vx) }.width(min:80,ideal:92) }
                    if columns.contains(.vy) { TableColumn("vy (\(session.unit)/s)") { row in cell(row,variable:.vy) }.width(min:80,ideal:92) }
                    if columns.contains(.speed) { TableColumn("v (\(session.unit)/s)") { row in cell(row,variable:.speed) }.width(min:80,ideal:92) }
                    if columns.contains(.ax) { TableColumn("ax (\(session.unit)/s²)") { row in cell(row,variable:.ax) }.width(min:80,ideal:92) }
                    if columns.contains(.ay) { TableColumn("ay (\(session.unit)/s²)") { row in cell(row,variable:.ay) }.width(min:80,ideal:92) }
                    TableColumn("Via") { row in
                        Text(row.observation.origin == .manual ? "M" : row.observation.origin == .automatic ? "A" : "R")
                            .help(row.observation.origin.rawValue)
                    }.width(30)
                }.font(.system(.caption,design:.monospaced))
                } else {
                    Table(data.rows) {
                        TableColumn("t (s)") { row in cell(row,variable:.time) }
                        TableColumn("x (\(session.unit))") { row in cell(row,variable:.x) }
                        TableColumn("y (\(session.unit))") { row in cell(row,variable:.y) }
                    }.font(.system(.caption,design:.monospaced))
                }
            }
            Divider()
            Text("— unavailable · M manual · A automatic · R reviewed")
                .font(.caption2).foregroundStyle(.secondary).padding(.top,4)
            HStack {
                Button("Undo",action:session.undo).disabled(!session.canUndo)
                Button("Redo",action:session.redo).disabled(!session.canRedo)
                Button("Remove Last",action:session.undoLastPoint).disabled(session.points.isEmpty)
                Spacer()
                Button("Export CSV…",action:session.exportCSV).disabled(session.points.isEmpty)
            }.controlSize(.mini).padding(10)
        }.frame(maxWidth:.infinity,maxHeight:.infinity)
    }
    private var supportsColumns: Bool {
        if #available(macOS 14.4, *) { return true }; return false
    }
    private func cell(_ row: MotionRow, variable: MotionVariable) -> some View {
        Group {
            if let value = variable.value(row) { Text(value,format:.number.precision(.fractionLength(variable == .time ? 3 : 4))) }
            else { Text("—").foregroundStyle(.secondary) }
        }.help(variable.value(row) == nil ? (row.unavailableReason ?? "Unavailable") : variable.title)
    }
}

struct CalibrationSheet: View {
    @ObservedObject var session: VideoSession
    @Environment(\.dismiss) private var dismiss
    @State private var length = "1.0"
    @State private var unit = "m"
    @State private var x1 = ""
    @State private var y1 = ""
    @State private var x2 = ""
    @State private var y2 = ""

    private var calibration: Calibration? {
        guard let number = Double(length), let ax = Double(x1), let ay = Double(y1),
              let bx = Double(x2), let by = Double(y2),
              [ax, ay, bx, by].allSatisfy({ $0.isFinite && $0 >= 0 }),
              ax < session.size.width, bx < session.size.width,
              ay < session.size.height, by < session.size.height else { return nil }
        let value = Calibration(first: CGPoint(x: ax, y: ay), second: CGPoint(x: bx, y: by), length: number, unit: unit)
        return value.isValid ? value : nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("Calibrate distance").font(.title2.weight(.semibold))
            Text("Enter the real distance between the two selected points.")
                .foregroundStyle(.secondary)
            Text("Endpoints in source pixels, from the top-left corner.")
                .font(.caption).foregroundStyle(.secondary)
            HStack {
                Text("Start x"); TextField("Start x", text: $x1)
                Text("y"); TextField("Start y", text: $y1)
            }
            HStack {
                Text("End x"); TextField("End x", text: $x2)
                Text("y"); TextField("End y", text: $y2)
            }
            Text("Known distance").font(.callout.weight(.medium))
            HStack {
                TextField("Known distance", text: $length).frame(width: 160)
                Picker("Unit", selection: $unit) {
                    Text("m").tag("m")
                    Text("cm").tag("cm")
                    Text("mm").tag("mm")
                }.frame(width: 110)
            }
            Text("Scale changes do not change your coordinate origin or axis angle.")
                .font(.caption).foregroundStyle(.secondary)
            HStack {
                Spacer()
                Button("Cancel") { session.calibrationPoints = []; dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("Apply Scale") {
                    if let calibration { try? session.applyCalibration(calibration) }
                    if let calibration { session.calibrationPoints = [calibration.first, calibration.second] }
                    session.tool = .navigate
                    dismiss()
                }.keyboardShortcut(.defaultAction).disabled(calibration == nil)
            }
        }.padding(24).frame(width: 420)
            .onAppear {
                let ends = session.calibrationPoints.count == 2 ? session.calibrationPoints :
                    [session.calibration?.first, session.calibration?.second].compactMap { $0 }
                if ends.count == 2 {
                    x1 = String(Double(ends[0].x)); y1 = String(Double(ends[0].y))
                    x2 = String(Double(ends[1].x)); y2 = String(Double(ends[1].y))
                }
                if let existing = session.calibration { length = String(existing.length); unit = existing.unit }
            }
    }
}

struct PositionEntrySheet: View {
    @ObservedObject var session: VideoSession
    @Environment(\.dismiss) private var dismiss
    @State private var x = ""
    @State private var y = ""
    private var point: CGPoint? {
        guard let px = Double(x), let py = Double(y), px.isFinite, py.isFinite,
              px >= 0, py >= 0, px < session.size.width, py < session.size.height else { return nil }
        return CGPoint(x: px, y: py)
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Enter position").font(.title2.weight(.semibold))
            Text("Source pixels from the top-left corner, at the current video time.")
                .font(.callout).foregroundStyle(.secondary)
            HStack {
                Text("x (px)"); TextField("x (px)", text: $x)
                Text("y (px)"); TextField("y (px)", text: $y)
            }
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Mark Position") {
                    if let point { session.selectTool(.mark); session.mark(point); dismiss() }
                }.keyboardShortcut(.defaultAction).disabled(point == nil || !session.canMark)
            }
        }.padding(24).frame(width: 420)
    }
}
