//! The system's appearance, light or dark, as the host reads it whenever it
//! colours a session, whether a window is open or not.

use adroit_protocol::ThemeKind;

/// The appearance the system is in now: on macOS as its preferences say,
/// switching with them, including while Auto switches by the time of day;
/// elsewhere dark.
pub fn system() -> ThemeKind {
    #[cfg(target_os = "macos")]
    {
        of_style(mac::interface_style().as_deref())
    }
    #[cfg(not(target_os = "macos"))]
    {
        ThemeKind::Dark
    }
}

/// The appearance macOS's `AppleInterfaceStyle` names: `Dark`, or no value
/// at all for light.
#[cfg(any(target_os = "macos", test))]
fn of_style(style: Option<&str>) -> ThemeKind {
    match style {
        Some("Dark") => ThemeKind::Dark,
        _ => ThemeKind::Light,
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use core_foundation::base::{CFType, CFTypeRef, TCFType};
    use core_foundation::string::{CFString, CFStringRef};

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFPreferencesAnyApplication: CFStringRef;
        fn CFPreferencesAppSynchronize(application: CFStringRef) -> u8;
        fn CFPreferencesCopyAppValue(key: CFStringRef, application: CFStringRef) -> CFTypeRef;
    }

    /// The global `AppleInterfaceStyle`, read afresh: a process that keeps
    /// running would otherwise go on reading the value it first cached.
    pub(super) fn interface_style() -> Option<String> {
        let key = CFString::from_static_string("AppleInterfaceStyle");
        // SAFETY: the static is CoreFoundation's own constant; the key
        // outlives both calls, and the value copied is owned here, released
        // when the wrapper drops.
        unsafe {
            CFPreferencesAppSynchronize(kCFPreferencesAnyApplication);
            let value =
                CFPreferencesCopyAppValue(key.as_concrete_TypeRef(), kCFPreferencesAnyApplication);
            if value.is_null() {
                return None;
            }
            CFType::wrap_under_create_rule(value)
                .downcast_into::<CFString>()
                .map(|s| s.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_is_dark_only_when_its_interface_style_says_dark() {
        assert_eq!(of_style(Some("Dark")), ThemeKind::Dark);
        assert_eq!(of_style(None), ThemeKind::Light);
        assert_eq!(of_style(Some("Light")), ThemeKind::Light);
    }
}
