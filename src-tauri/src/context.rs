// SmartSearch — Active Context Detection (macOS)
// Detects the foreground application using NSWorkspace API

/// Get the name of the currently focused/foreground application
#[cfg(target_os = "macos")]
pub fn get_foreground_app() -> Option<String> {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::NSString;

    unsafe {
        let workspace = NSWorkspace::sharedWorkspace();
        let front_app = workspace.frontmostApplication()?;
        let name = front_app.localizedName()?;
        Some(name.to_string())
    }
}

#[cfg(not(target_os = "macos"))]
pub fn get_foreground_app() -> Option<String> {
    // Stub for non-macOS platforms
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_foreground_app() {
        // This test only verifies the function doesn't panic
        // On macOS it should return Some(app_name)
        // On other platforms it returns None
        let _result = get_foreground_app();
    }
}
