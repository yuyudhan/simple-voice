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

// llama.cpp is only published as a dynamic framework. The app bundle carries it in
// Contents/Frameworks (tauri.conf.json `bundle.macOS.frameworks`) and the helper sits in
// Contents/MacOS; `tauri dev` runs the helper from target/debug, so debug builds also look in
// src-tauri/binaries, where engine/build.sh puts the framework.
func runtimeSearchPath(_ path: String, _ configuration: BuildConfiguration? = nil) -> LinkerSetting {
    .unsafeFlags(["-Xlinker", "-rpath", "-Xlinker", path], configuration.map { .when(configuration: $0) })
}

let devFrameworks = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .appendingPathComponent("src-tauri/binaries")
    .path

let package = Package(
    name: "SimpleVoiceEngine",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "simple-voice-engine", targets: ["SimpleVoiceEngine"])
    ],
    dependencies: [
        .package(url: "https://github.com/FluidInference/FluidAudio.git", exact: "0.17.4"),
        .package(url: "https://github.com/argmaxinc/WhisperKit.git", exact: "1.1.0"),
    ],
    targets: [
        .executableTarget(
            name: "SimpleVoiceEngine",
            dependencies: [
                .product(name: "FluidAudio", package: "FluidAudio"),
                .product(name: "WhisperKit", package: "WhisperKit"),
                "llama",
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
                runtimeSearchPath("@executable_path/../Frameworks"),
                runtimeSearchPath(devFrameworks, .debug),
            ]
        ),
        // llama.cpp b11575 (MIT), the official Apple XCFramework release asset. b11576 ("graph: pad
        // the KV dimension of the no-cache attention to 256", #30416) breaks T5 encoders, which the
        // MADLAD translator needs: its output turns into repeated fragments. Do not move past it
        // without re-running the translation smoke.
        .binaryTarget(
            name: "llama",
            url: "https://github.com/ggml-org/llama.cpp/releases/download/b11575/llama-b11575-xcframework.zip",
            checksum: "9234c47e6b1f29fb8a3b6466647bff5d82a6113a41dc5dcfc47643d7da62c4ea"
        ),
    ]
)
