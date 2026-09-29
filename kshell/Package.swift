// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "kshell",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .executable(name: "kshell", targets: ["kshell"]),
        .library(name: "KShellCore", targets: ["KShellCore"]),
    ],
    dependencies: [
        .package(url: "https://github.com/LebJe/TOMLKit.git", from: "0.6.0"),
    ],
    targets: [
        .target(
            name: "KShellCore",
            dependencies: [
                .product(name: "TOMLKit", package: "TOMLKit"),
            ],
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
        .executableTarget(
            name: "kshell",
            dependencies: ["KShellCore"],
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
        .testTarget(
            name: "KShellCoreTests",
            dependencies: ["KShellCore"],
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
    ]
)
