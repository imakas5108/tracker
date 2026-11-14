import SwiftUI
import AVKit

struct WorkspaceView: View {
    @ObservedObject var workspace: Workspace

    var body: some View {
        VStack(spacing: 0) {
            if !workspace.sessions.isEmpty {
                HStack(spacing: 0) {
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 0) {
                            ForEach(workspace.sessions) { session in
                                HStack(spacing: 8) {
                                    Button { workspace.select(session.id) } label: {
                                        Label(session.name, systemImage: "film")
                                            .lineLimit(1).frame(maxWidth: 190)
                                    }
                                    .buttonStyle(.plain)
                                    .accessibilityAddTraits(workspace.selectedID == session.id ? .isSelected : [])
                                    Button { workspace.close(session.id) } label: {
                                        Image(systemName: "xmark").font(.system(size: 9, weight: .semibold))
                                    }
                                    .buttonStyle(.borderless).help("Close \(session.name)")
                                    .accessibilityLabel("Close \(session.name)")
                                }
                                .padding(.horizontal, 14).frame(height: 36)
                                .background(workspace.selectedID == session.id ? Color(nsColor: .controlBackgroundColor) : .clear)
                                Divider().frame(height: 18)
                            }
                        }
                    }
                    Text("\(workspace.sessions.count) video\(workspace.sessions.count == 1 ? "" : "s")")
                        .font(.caption).foregroundStyle(.secondary).padding(.horizontal, 14)
                }
                .background(Color(nsColor: .windowBackgroundColor))
                Divider()
            }
            if let session = workspace.selected {
                SessionView(session: session).id(session.id)
            } else {
                EmptyWorkspace(open: workspace.openVideos)
            }
        }
        .toolbar {
            ToolbarItem(placement: .navigation) {
                Button(action: workspace.openVideos) { Label("Open Video", systemImage: "folder") }
                    .help("Open one or more videos (⌘O)")
            }
            ToolbarItem(placement: .principal) {
                Text("Tracker").font(.headline)
            }
        }
    }
}

struct EmptyWorkspace: View {
    var open: () -> Void
    var body: some View {
        HSplitView {
            VStack(alignment: .leading, spacing: 18) {
                Text("Tracks").font(.headline)
                Text("Your point tracks and calibration will appear here.")
                    .font(.callout).foregroundStyle(.secondary)
                Spacer()
            }.padding(18).frame(width: 190, alignment: .topLeading).frame(maxHeight: .infinity)
            VStack(spacing: 0) {
                Spacer()
                VStack(spacing: 16) {
                    Image(systemName: "film").font(.system(size: 36, weight: .light)).foregroundStyle(.secondary)
                    Text("Open a video to begin").font(.title2.weight(.medium))
                    Text("Set a scale, mark positions, and inspect the motion.")
                        .foregroundStyle(.secondary)
                    Button("Open Video…", action: open).keyboardShortcut(.defaultAction)
                    Text("MOV, MP4, and other formats supported by macOS")
                        .font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                Text("Video analysis workspace · Interface prototype")
                    .font(.caption).foregroundStyle(.secondary).padding(16)
            }.frame(maxWidth: .infinity, maxHeight: .infinity)
            VStack(spacing: 0) {
                PanelHeader(title: "Plot", symbol: "chart.xyaxis.line")
                ContentUnavailableView("No track data", systemImage: "chart.xyaxis.line",
                                       description: Text("Mark positions to plot motion over time."))
                Divider()
                PanelHeader(title: "Data", symbol: "tablecells")
                ContentUnavailableView("No measurements", systemImage: "tablecells",
                                       description: Text("Time and coordinates will appear here."))
            }.frame(minWidth: 260, idealWidth: 300, maxWidth: 400, maxHeight: .infinity)
        }.frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

struct SessionView: View {
    @ObservedObject var session: VideoSession
    @State private var showPositionEntry = false

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 14) {
                HStack(spacing: 6) {
                    ForEach(WorkspaceTool.allCases, id: \.self) { tool in
                        Button { session.selectTool(tool) } label: {
                            Label(tool.rawValue, systemImage: tool.symbol)
                        }
                        .tint(session.tool == tool ? .accentColor : nil)
                        .accessibilityAddTraits(session.tool == tool ? .isSelected : [])
                    }
                }.buttonStyle(.bordered)
                Toggle(isOn: $session.showAxes) { Label("Axes", systemImage: "arrow.up.and.right") }
                    .toggleStyle(.button).help("Show the coordinate origin at the lower left of the video")
                Divider().frame(height: 20)
                Label("Autotracker: not implemented", systemImage: "scope")
                    .font(.callout).foregroundStyle(.secondary)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 14).padding(.vertical, 10)
            .disabled(session.loading || session.error != nil)
            Divider()
            HSplitView {
                trackList.frame(minWidth: 170, idealWidth: 190, maxWidth: 220)
                VStack(spacing: 0) {
                    HStack {
                        Text(session.selectedTrack?.name ?? "Video").font(.callout.weight(.medium))
                        Spacer()
                        Text("\(Int(session.size.width)) × \(Int(session.size.height)) px")
                            .font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                    }.padding(.horizontal, 14).frame(height: 36)
                    VideoCanvas(session: session)
                    PlaybackBar(session: session)
                }.frame(minWidth: 420, maxWidth: .infinity, maxHeight: .infinity).layoutPriority(1)
                VSplitView {
                    PositionPlot(session: session).frame(minHeight: 200)
                    DataPanel(session: session).frame(minHeight: 180)
                }.frame(minWidth: 270, idealWidth: 290, maxWidth: 420)
            }
            Divider()
            HStack {
                Text(session.status).lineLimit(1)
                Spacer()
                Text("\(session.points.count) \(session.points.count == 1 ? "point" : "points") · \(session.unit)").monospacedDigit()
            }.font(.caption).foregroundStyle(.secondary).padding(.horizontal, 14).frame(height: 28)
        }
        .sheet(isPresented: $session.showCalibration) { CalibrationSheet(session: session) }
        .sheet(isPresented: $showPositionEntry) { PositionEntrySheet(session: session) }
        .alert("Could not export", isPresented: Binding(get: { session.exportError != nil }, set: { if !$0 { session.exportError = nil } })) {
            Button("OK") { session.exportError = nil }
        } message: { Text(session.exportError ?? "") }
    }

    private var trackList: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Tracks").font(.headline)
                Spacer()
                Button(action: session.addTrack) { Image(systemName: "plus") }
                    .help("Add a point mass").accessibilityLabel("Add a point mass")
                    .disabled(session.loading || session.error != nil)
            }.padding(14)
            if session.tracks.isEmpty {
                Text("Add a point mass to start marking positions.")
                    .font(.callout).foregroundStyle(.secondary).padding(.horizontal, 14)
            } else {
                List(selection: $session.selectedTrackID) {
                    ForEach(session.tracks) { track in
                        Label(track.name, systemImage: "plus.circle").tag(track.id)
                    }
                }.listStyle(.sidebar)
            }
            Spacer(minLength: 16)
            if session.selectedTrack != nil {
                Button("Enter Position…") { session.pause(); showPositionEntry = true }
                    .padding(14).keyboardShortcut("m", modifiers: [.command, .shift])
            }
            Divider()
            VStack(alignment: .leading, spacing: 8) {
                Label("Scale", systemImage: "ruler").font(.callout.weight(.semibold))
                if let calibration = session.calibration {
                    Text("\(calibration.pixelsPerUnit, specifier: "%.2f") px / \(calibration.unit)")
                        .font(.callout.monospacedDigit())
                    Button("Reset Scale") { session.calibration = nil; session.calibrationPoints = [] }
                } else {
                    Text("Not calibrated").font(.callout)
                    Text("Use Calibrate to select a known distance.")
                        .font(.caption).foregroundStyle(.secondary)
                }
                Button("Set Scale…") { session.pause(); session.showCalibration = true }
                    .disabled(session.loading || session.error != nil)
                Text("Origin: bottom left\nx right · y up")
                    .font(.caption).foregroundStyle(.secondary).padding(.top, 8)
            }.padding(14)
        }.background(Color(nsColor: .windowBackgroundColor))
    }
}

struct PanelHeader: View {
    var title: String
    var symbol: String
    var body: some View {
        HStack {
            Label(title, systemImage: symbol).font(.callout.weight(.semibold))
            Spacer()
        }.padding(.horizontal, 12).frame(height: 36)
            .background(Color(nsColor: .windowBackgroundColor))
    }
}

struct VideoCanvas: View {
    @ObservedObject var session: VideoSession
    var body: some View {
        GeometryReader { geometry in
            let sourceSize = session.size
            let rect = VideoGeometry.fittedRect(source: sourceSize, container: geometry.size)
            ZStack {
                Color(nsColor: NSColor(white: 0.09, alpha: 1))
                if session.loading {
                    ProgressView("Opening video…").tint(.white).foregroundStyle(.white)
                } else if let error = session.error {
                    VStack(spacing: 12) {
                        Image(systemName: "exclamationmark.triangle").font(.title)
                        Text(error).multilineTextAlignment(.center)
                    }.padding(28).foregroundStyle(.white)
                } else {
                    NativeVideo(player: session.player)
                    Canvas { context, _ in
                        func project(_ point: CGPoint) -> CGPoint {
                            CGPoint(x: rect.minX + point.x / sourceSize.width * rect.width,
                                    y: rect.minY + point.y / sourceSize.height * rect.height)
                        }
                        if session.showAxes {
                            let origin = CGPoint(x: rect.minX + 1, y: rect.maxY - 1)
                            var axes = Path()
                            axes.move(to: CGPoint(x: origin.x, y: rect.minY)); axes.addLine(to: origin)
                            axes.addLine(to: CGPoint(x: rect.maxX, y: origin.y))
                            context.stroke(axes, with: .color(.black), lineWidth: 4)
                            context.stroke(axes, with: .color(.white), lineWidth: 1)
                            context.fill(Path(CGRect(x: rect.maxX - 21, y: rect.maxY - 21, width: 19, height: 19)), with: .color(.black))
                            context.fill(Path(CGRect(x: rect.minX + 2, y: rect.minY + 2, width: 19, height: 19)), with: .color(.black))
                            context.draw(Text("x").foregroundColor(.white), at: CGPoint(x: rect.maxX - 12, y: rect.maxY - 12))
                            context.draw(Text("y").foregroundColor(.white), at: CGPoint(x: rect.minX + 12, y: rect.minY + 12))
                        }
                        for point in session.points {
                            let center = project(point.pixel)
                            var cross = Path()
                            cross.move(to: CGPoint(x: center.x - 6, y: center.y)); cross.addLine(to: CGPoint(x: center.x + 6, y: center.y))
                            cross.move(to: CGPoint(x: center.x, y: center.y - 6)); cross.addLine(to: CGPoint(x: center.x, y: center.y + 6))
                            context.stroke(cross, with: .color(.black), lineWidth: 4)
                            context.stroke(cross, with: .color(.white), lineWidth: 2)
                        }
                        let ends = session.calibrationPoints.isEmpty ? [session.calibration?.first, session.calibration?.second].compactMap { $0 } : session.calibrationPoints
                        if let first = ends.first {
                            var line = Path()
                            line.move(to: project(first))
                            if ends.count > 1 { line.addLine(to: project(ends[1])) }
                            context.stroke(line, with: .color(.black), lineWidth: 5)
                            context.stroke(line, with: .color(.white), lineWidth: 2)
                            for point in ends {
                                let center = project(point)
                                context.fill(Path(ellipseIn: CGRect(x: center.x - 4, y: center.y - 4, width: 8, height: 8)), with: .color(.white))
                            }
                        }
                    }.allowsHitTesting(false)
                    if session.tool != .navigate {
                        Color.clear.contentShape(Rectangle())
                            .onTapGesture { location in
                                if let point = VideoGeometry.sourcePoint(location, source: session.size, rect: rect) { session.mark(point) }
                            }
                            .accessibilityLabel(session.tool == .mark ? "Video marking area" : "Calibration endpoint area")
                    }
                }
            }
        }.clipped()
    }
}

struct NativeVideo: NSViewRepresentable {
    var player: AVPlayer
    func makeNSView(context: Context) -> AVPlayerView {
        let view = AVPlayerView()
        view.controlsStyle = .none
        view.videoGravity = .resizeAspect
        view.player = player
        return view
    }
    func updateNSView(_ view: AVPlayerView, context: Context) { view.player = player }
}

struct PlaybackBar: View {
    @ObservedObject var session: VideoSession
    var body: some View {
        VStack(spacing: 10) {
            Slider(value: Binding(get: { session.time }, set: { session.pause(); session.seek($0) }),
                   in: 0...max(session.duration, 0.01))
                .accessibilityLabel("Video time")
            HStack(spacing: 16) {
                Button { session.seek(0); session.pause() } label: { Image(systemName: "backward.end.fill") }
                    .help("Go to start").accessibilityLabel("Go to start")
                Button { session.step(-1) } label: { Image(systemName: "backward.frame") }
                    .help("Previous frame").accessibilityLabel("Previous frame")
                Button(action: session.togglePlayback) {
                    Image(systemName: session.playing ? "pause.fill" : "play.fill").frame(width: 16)
                }.help("Play or pause (Space)").accessibilityLabel(session.playing ? "Pause" : "Play")
                    .keyboardShortcut(.space, modifiers: [])
                Button { session.step(1) } label: { Image(systemName: "forward.frame") }
                    .help("Next frame").accessibilityLabel("Next frame")
                Spacer()
                Text("\(session.time, specifier: "%.3f") / \(session.duration, specifier: "%.3f") s")
                    .font(.system(.callout, design: .monospaced)).monospacedDigit()
            }.buttonStyle(.borderless)
        }.padding(14).background(Color(nsColor: .windowBackgroundColor))
            .disabled(session.loading || session.error != nil)
    }
}
