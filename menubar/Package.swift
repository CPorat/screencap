// swift-tools-version: 6.3
import PackageDescription

let package = Package(
    name: "ScreencapMenu",
    platforms: [
        .macOS(.v13)
    ],
    targets: [
        .executableTarget(
            name: "ScreencapMenu"
        ),
        .testTarget(
            name: "ScreencapMenuTests",
            dependencies: ["ScreencapMenu"]
        )
    ],
    swiftLanguageModes: [.v6]
)
