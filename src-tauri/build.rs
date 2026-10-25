fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "subscribe",
            "kill_agent",
            "report_ui_metrics",
            "set_debug_stream",
            "ui_log",
        ]),
    ))
    .expect("tauri build");
}
