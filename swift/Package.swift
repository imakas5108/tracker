// swift-tools-version: 6.1
import PackageDescription

let package = Package(
    name: "Tracker",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "Tracker", targets: ["Tracker"]),
        .executable(name: "TrackerMCP", targets: ["TrackerMCP"])
    ],
    dependencies: [
        .package(url: "https://github.com/modelcontextprotocol/swift-sdk.git", exact: "0.11.0"),
        // Keep the resolved graph usable by Swift 6.1; newer releases raise the tools floor.
        .package(url: "https://github.com/apple/swift-system.git", exact: "1.4.2"),
        .package(url: "https://github.com/apple/swift-log.git", exact: "1.6.4"),
        .package(url: "https://github.com/apple/swift-async-algorithms.git", exact: "1.0.4"),
        .package(url: "https://github.com/apple/swift-collections.git", exact: "1.2.1")
    ],
    targets: [
        .target(name: "TrackerAutomation"),
        .executableTarget(name: "Tracker", dependencies: ["TrackerAutomation"]),
        .executableTarget(name: "TrackerMCP", dependencies: ["TrackerAutomation", .product(name: "MCP", package: "swift-sdk"), .product(name: "Logging", package: "swift-log")]),
        .testTarget(name: "TrackerTests", dependencies: ["Tracker", "TrackerAutomation"]),
        .testTarget(name: "TrackerAutomationTests", dependencies: ["TrackerAutomation"])
    ],
    swiftLanguageModes: [.v5]
)
