fn main() {
    #[cfg(target_os = "windows")]
    {
        tauri_build::build()
            .windows_subsystem("windows")
            .expect("failed to set windows subsystem");
    }
    #[cfg(not(target_os = "windows"))]
    {
        tauri_build::build()
    }
}
