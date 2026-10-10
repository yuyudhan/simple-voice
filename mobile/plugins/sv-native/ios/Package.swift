// swift-tools-version:5.9
// FilePath: mobile/plugins/sv-native/ios/Package.swift
// Built by the plugin's build script (swift-rs) into the app's Rust library. SvShared is also
// compiled straight into the keyboard and widget targets (see gen/apple/project.yml).

import PackageDescription

let package = Package(
    name: "tauri-plugin-sv-native",
    platforms: [.iOS(.v17)],
    products: [
        .library(name: "tauri-plugin-sv-native", type: .static, targets: ["tauri-plugin-sv-native"])
    ],
    dependencies: [
        .package(name: "Tauri", path: "../.tauri/tauri-api")
    ],
    targets: [
        .target(name: "SvShared", path: "Sources/SvShared"),
        .target(
            name: "tauri-plugin-sv-native",
            dependencies: [.byName(name: "Tauri"), "SvShared"],
            path: "Sources/SvNative"
        ),
    ]
)
