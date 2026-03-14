// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "VelocitySyncProbe",
    platforms: [.iOS(.v14)],
    products: [
        .library(
            name: "VelocitySyncProbe",
            targets: ["VelocitySyncProbe"]
        ),
    ],
    targets: [
        .target(
            name: "VelocitySyncProbe",
            path: "Sources"
        ),
    ]
)
