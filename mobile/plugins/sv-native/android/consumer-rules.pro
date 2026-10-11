# FilePath: mobile/plugins/sv-native/android/consumer-rules.pro
# Tauri finds the plugin and its commands by reflection, so minified app builds must keep them.
-keep class dev.yuyudhan.simplevoice.svnative.SvNativePlugin { *; }
