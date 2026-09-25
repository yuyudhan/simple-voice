// swift-tools-version: 6.0
// FilePath: engine/Package.swift
import Foundation
import PackageDescription

// The helper is a bare executable, not a bundle, so the privacy usage strings that the Speech and
// AVFoundation permission prompts require are embedded into the binary's __TEXT,__info_plist
// section; `Bundle.main.infoDictionary` reads them from there.
let infoPlistPath = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .appendingPathComponent("Info.plist")
    .path

let package = Package(
    name: "SimpleVoiceEngine",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "simple-voice-engine", targets: ["SimpleVoiceEngine"])
    ],
    dependencies: [
        .package(url: "https://github.com/FluidInference/FluidAudio.git", exact: "0.17.4")
    ],
    targets: [
        .executableTarget(
            name: "SimpleVoiceEngine",
            dependencies: [
                .product(name: "FluidAudio", package: "FluidAudio")
            ],
            path: "Sources/SimpleVoiceEngine",
            swiftSettings: [
                .swiftLanguageMode(.v6),
                // FoundationModels only exists on macOS 26+. Autolinking would add a strong load
                // command and the binary would fail to launch on macOS 14/15, so the framework is
                // linked weakly by hand below instead.
                .unsafeFlags(["-Xfrontend", "-disable-autolink-framework", "-Xfrontend", "FoundationModels"]),
            ],
            linkerSettings: [
                .unsafeFlags([
                    "-Xlinker", "-weak_framework", "-Xlinker", "FoundationModels",
                    "-Xlinker", "-sectcreate", "-Xlinker", "__TEXT", "-Xlinker", "__info_plist",
                    "-Xlinker", infoPlistPath,
                ])
            ]
        )
    ]
)
