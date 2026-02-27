// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "OpenClipboard",
    platforms: [
        .macOS(.v13)
    ],
    products: [
        .executable(name: "OpenClipboard", targets: ["OpenClipboard"])
    ],
    targets: [
        .executableTarget(
            name: "OpenClipboard",
            dependencies: [],
            path: "OpenClipboard"
        )
    ]
)