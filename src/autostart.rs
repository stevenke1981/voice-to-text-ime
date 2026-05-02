use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
use winreg::RegKey;

const APP_NAME: &str = "VoiceToTextIME";
const RUN_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run";

pub fn get() -> bool {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(RUN_KEY)
        .and_then(|k| k.get_value::<String, _>(APP_NAME))
        .is_ok()
}

pub fn set(enabled: bool) -> std::io::Result<()> {
    let run = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE)?;
    if enabled {
        let path = std::env::current_exe()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        run.set_value(APP_NAME, &path)
    } else {
        // delete_value returns Err if key doesn't exist; that's fine
        let _ = run.delete_value(APP_NAME);
        Ok(())
    }
}
