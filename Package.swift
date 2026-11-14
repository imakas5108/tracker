// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "Tracker",
    platforms: [.macOS(.v14)],
    products: [.executable(name: "Tracker", targets: ["Tracker"])],
    targets: [
        .executableTarget(name: "Tracker"),
        .testTarget(name: "TrackerTests", dependencies: ["Tracker"])
    ],
    swiftLanguageModes: [.v5]
)
