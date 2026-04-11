// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "ScreencapMenubar",
    platforms: [
        .macOS(.v13)
    ],
    products: [
        .executable(
            name: "ScreencapMenubar",
            targets: ["ScreencapMenubar"]
        )
    ],
    targets: [
        .executableTarget(
            name: "ScreencapMenubar",
            path: ".",
            exclude: ["Package.swift"],
            sources: ["ScreencapApp.swift"]
        )
    ]
)
