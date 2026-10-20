//! `.md` file association: kknow as the default app for markdown.
//!
//! `Info.plist` (`CFBundleDocumentTypes`) declares that kknow can open
//! markdown. This puts kknow in "Open With". But the system lets only the user
//! make it the double-click handler ("Get Info → Open with → Change All"), or
//! Launch Services. The settings button uses Launch Services, so we do not
//! have to explain the manual steps.
//!
//! Launch Services is a C API, so this file uses `unsafe`. Two functions wrap
//! it and expose plain strings. Apple marks these functions as deprecated (the
//! replacement, `NSWorkspace`, is Objective-C and one more layer), but they
//! work. When they stop working, only this file must change.

use serde::Serialize;
use tauri::{AppHandle, Runtime};

/// The system content type for markdown: `.md`, `.markdown`, `.mdown`.
pub const MARKDOWN_TYPE: &str = "net.daringfireball.markdown";

/// Which app opens markdown now.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Association {
    /// The platform lets the app set the default handler from here.
    supported: bool,
    /// Bundle ID of the app that opens `.md` now.
    current: Option<String>,
    /// The current handler is kknow.
    ours: bool,
}

fn association<R: Runtime>(app: &AppHandle<R>) -> Association {
    let current = imp::default_handler(MARKDOWN_TYPE);
    let ours = current
        .as_deref()
        .is_some_and(|id| id.eq_ignore_ascii_case(&app.config().identifier));
    Association {
        supported: imp::SUPPORTED,
        current,
        ours,
    }
}

#[tauri::command]
pub fn markdown_association<R: Runtime>(app: AppHandle<R>) -> Association {
    association(&app)
}

/// Makes kknow the default app for markdown.
///
/// Works only for the installed app. Launch Services knows only apps in
/// `/Applications` (or started from there). It does not know a dev build and
/// returns an error.
#[tauri::command]
pub fn claim_markdown<R: Runtime>(app: AppHandle<R>) -> Result<Association, String> {
    imp::set_default_handler(MARKDOWN_TYPE, &app.config().identifier)?;
    Ok(association(&app))
}

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::{CStr, CString, c_char, c_void};

    pub const SUPPORTED: bool = true;

    type CFStringRef = *const c_void;
    type CFAllocatorRef = *const c_void;
    type CFIndex = isize;
    type OSStatus = i32;

    const UTF8: u32 = 0x0800_0100;
    /// `kLSRolesAll`: both viewing and editing, on double click.
    const ROLES_ALL: u32 = 0xFFFF_FFFF;
    /// `kLSApplicationNotFoundErr`: Launch Services does not know the app.
    const NOT_FOUND: OSStatus = -10814;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringCreateWithCString(
            alloc: CFAllocatorRef,
            text: *const c_char,
            encoding: u32,
        ) -> CFStringRef;
        fn CFStringGetCString(
            text: CFStringRef,
            buffer: *mut c_char,
            size: CFIndex,
            encoding: u32,
        ) -> u8;
        fn CFRelease(object: *const c_void);
    }

    #[link(name = "CoreServices", kind = "framework")]
    unsafe extern "C" {
        fn LSCopyDefaultRoleHandlerForContentType(
            content_type: CFStringRef,
            role: u32,
        ) -> CFStringRef;
        fn LSSetDefaultRoleHandlerForContentType(
            content_type: CFStringRef,
            role: u32,
            handler: CFStringRef,
        ) -> OSStatus;
    }

    /// A Core Foundation string that releases itself on drop.
    struct CfString(CFStringRef);

    impl CfString {
        fn new(text: &str) -> Option<Self> {
            let text = CString::new(text).ok()?;
            // SAFETY: `text` is a live NUL-terminated string; the allocator is
            // the default (`NULL`); the result is checked for `NULL` below.
            let raw = unsafe { CFStringCreateWithCString(std::ptr::null(), text.as_ptr(), UTF8) };
            (!raw.is_null()).then_some(CfString(raw))
        }

        fn to_string(&self) -> Option<String> {
            let mut buffer = [0 as c_char; 512];
            // SAFETY: the string is live, we own the buffer, and we pass its
            // real length.
            let done = unsafe {
                CFStringGetCString(self.0, buffer.as_mut_ptr(), buffer.len() as CFIndex, UTF8)
            };
            if done == 0 {
                return None;
            }
            // SAFETY: on success the function wrote a NUL-terminated string.
            let text = unsafe { CStr::from_ptr(buffer.as_ptr()) };
            Some(text.to_string_lossy().into_owned())
        }
    }

    impl Drop for CfString {
        fn drop(&mut self) {
            // SAFETY: we created the string (Create/Copy), and we release it
            // exactly once.
            unsafe { CFRelease(self.0) }
        }
    }

    pub fn default_handler(content_type: &str) -> Option<String> {
        let kind = CfString::new(content_type)?;
        // SAFETY: `kind` lives until the end of the function. We own the
        // result of the Copy function, and `CfString` releases it.
        let raw = unsafe { LSCopyDefaultRoleHandlerForContentType(kind.0, ROLES_ALL) };
        if raw.is_null() {
            return None;
        }
        CfString(raw).to_string()
    }

    pub fn set_default_handler(content_type: &str, bundle: &str) -> Result<(), String> {
        let kind = CfString::new(content_type).ok_or("негодный тип содержимого")?;
        let handler = CfString::new(bundle).ok_or("негодный идентификатор приложения")?;
        // SAFETY: both strings live until the call ends.
        let status = unsafe { LSSetDefaultRoleHandlerForContentType(kind.0, ROLES_ALL, handler.0) };
        match status {
            0 => Ok(()),
            NOT_FOUND => Err(format!(
                "система не знает приложение {bundle}: назначить его можно только \
                 установленному — pnpm install:app"
            )),
            other => Err(format!("Launch Services отказали (код {other})")),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Read only. Tests must not set the default app: that is a setting of
        /// the person who runs them.
        #[test]
        fn strings_survive_the_round_trip() {
            let text = CfString::new("net.daringfireball.markdown").unwrap();
            assert_eq!(text.to_string().unwrap(), "net.daringfireball.markdown");
        }

        #[test]
        fn asking_for_an_unknown_type_is_harmless() {
            assert!(default_handler("dev.kknow.no-such-type").is_none());
        }
    }
}

/// On other systems the installer sets the file association, not the app.
#[cfg(not(target_os = "macos"))]
mod imp {
    pub const SUPPORTED: bool = false;

    pub fn default_handler(_content_type: &str) -> Option<String> {
        None
    }

    pub fn set_default_handler(_content_type: &str, _bundle: &str) -> Result<(), String> {
        Err("назначить программу по умолчанию отсюда можно только на macOS".into())
    }
}
