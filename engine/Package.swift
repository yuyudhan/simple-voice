// swift-tools-version: 6.0
// FilePath: engine/Package.swift
import Foundation
import PackageDescription

// The helper is a bare executable, not a bundle, so the privacy usage strings that the Speech and
// AVFoundation permission prompts require are embedded into the binary's __TEXT,__info_plist
// section; `Bundle.main.infoDictionary` reads them from there. Debug builds (what `just dev` runs)
// embed Info.dev.plist instead, so a development helper is "Simple Voice Dev" with its own
// identifier and can never share a privacy grant with the installed app.
func infoPlistPath(_ name: String) -> String {
    URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent()
        .appendingPathComponent(name)
        .path
}

func embedInfoPlist(_ name: String, _ configuration: BuildConfiguration) -> LinkerSetting {
    .unsafeFlags(
        ["-Xlinker", "-sectcreate", "-Xlinker", "__TEXT", "-Xlinker", "__info_plist", "-Xlinker", infoPlistPath(name)],
        .when(configuration: configuration)
    )
}

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
                .unsafeFlags(["-Xlinker", "-weak_framework", "-Xlinker", "FoundationModels"]),
                embedInfoPlist("Info.plist", .release),
                embedInfoPlist("Info.dev.plist", .debug),
            ]
        )
    ]
)
