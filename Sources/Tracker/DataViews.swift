import SwiftUI
import Charts

struct PositionPlot: View {
    @ObservedObject var session: VideoSession
    @State private var component = "x"
    var body: some View {
        VStack(spacing: 0) {
            PanelHeader(title: "Position", symbol: "chart.xyaxis.line")
            Divider()
            HStack {
                Text("\(component) (\(session.unit))").font(.caption).foregroundStyle(.secondary)
                Spacer()
                HStack(spacing: 4) {
                    ForEach(["x", "y"], id: \.self) { axis in
                        Button(axis) { component = axis }
                            .tint(component == axis ? .accentColor : nil)
                            .accessibilityLabel("Plot \(axis) position")
                            .accessibilityAddTraits(component == axis ? .isSelected : [])
                    }
                }.buttonStyle(.bordered).controlSize(.small)
            }.padding(12)
            if session.points.isEmpty {
                VStack(spacing: 8) {
                    Text("No positions marked").font(.callout.weight(.medium))
                    Text("Mark points in the video to plot \(component) against time.")
                        .font(.caption).foregroundStyle(.secondary).multilineTextAlignment(.center)
                }.padding(24).frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                Chart(session.points) { point in
                    let position = session.coordinates(point.pixel)
                    PointMark(x: .value("Time", point.time), y: .value(component, Double(component == "x" ? position.x : position.y)))
                        .foregroundStyle(Color.accentColor)
                }
                .chartXAxisLabel("Time (s)")
                .padding(.horizontal, 16).padding(.bottom, 16)
                .accessibilityLabel("\(component) position versus time for \(session.selectedTrack?.name ?? "track")")
            }
        }.frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

struct DataPanel: View {
    @ObservedObject var session: VideoSession
    var body: some View {
        VStack(spacing: 0) {
            PanelHeader(title: "Data", symbol: "tablecells")
            Divider()
            if session.points.isEmpty {
                VStack(spacing: 8) {
                    Text("No measurements yet").font(.callout.weight(.medium))
                    Text("Recorded time and coordinates will appear here.")
                        .font(.caption).foregroundStyle(.secondary).multilineTextAlignment(.center)
                }.padding(24).frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                Table(session.points) {
                    TableColumn("t (s)") { point in Text(point.time, format: .number.precision(.fractionLength(3))) }
                        .width(min: 55, ideal: 70, max: 130)
                    TableColumn("x (\(session.unit))") { point in Text(session.coordinates(point.pixel).x, format: .number.precision(.fractionLength(2))) }
                        .width(min: 55, ideal: 70, max: 130)
                    TableColumn("y (\(session.unit))") { point in Text(session.coordinates(point.pixel).y, format: .number.precision(.fractionLength(2))) }
                        .width(min: 55, ideal: 70, max: 130)
                }.font(.system(.caption, design: .monospaced))
            }
            Divider()
            HStack {
                Button("Remove Last", action: session.undoLastPoint)
                    .help("Remove the point with the latest timestamp")
                Spacer()
                Button("Export CSV…", action: session.exportCSV)
            }.controlSize(.small).padding(10).disabled(session.points.isEmpty)
        }.frame(maxWidth: .infinity, maxHeight: .infinity)
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
            Text("The coordinate origin stays at the bottom-left corner of the video.")
                .font(.caption).foregroundStyle(.secondary)
            HStack {
                Spacer()
                Button("Cancel") { session.calibrationPoints = []; dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("Apply Scale") {
                    session.calibration = calibration
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
                }.keyboardShortcut(.defaultAction).disabled(point == nil)
            }
        }.padding(24).frame(width: 420)
    }
}
