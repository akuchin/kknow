use serde::Serialize;
use tauri::{AppHandle, Runtime};

use crate::errors::ErrorView;

pub const MARKDOWN_TYPE: &str = "net.daringfireball.markdown";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS), ts(rename = "MarkdownAssociation"))]
pub struct Association {
    supported: bool,
    current: Option<String>,
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

#[tauri::command(async)]
pub fn markdown_association<R: Runtime>(app: AppHandle<R>) -> Association {
    association(&app)
}

#[tauri::command(async)]
pub fn claim_markdown<R: Runtime>(app: AppHandle<R>) -> Result<Association, ErrorView> {
    imp::set_default_handler(MARKDOWN_TYPE, &app.config().identifier).map_err(Refusal::view)?;
    Ok(association(&app))
}

#[derive(Debug)]
#[allow(dead_code)]
enum Refusal {
    Unsupported,
    BadName(String),
    NotInstalled(String),
    Status(i32),
}

impl Refusal {
    fn view(self) -> ErrorView {
        let message = self.to_string();
        let (code, value) = match self {
            Refusal::Unsupported => ("associationUnsupported", None),
            Refusal::BadName(name) => ("associationBadName", Some(name)),
            Refusal::NotInstalled(bundle) => ("associationNotInstalled", Some(bundle)),
            Refusal::Status(status) => ("associationStatus", Some(status.to_string())),
        };
        ErrorView {
            code,
            params: value.map(|value| ("value", value)).into_iter().collect(),
            message,
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::Unsupported => write!(f, "only macOS lets kknow set the default app"),
            Refusal::BadName(name) => write!(f, "invalid name: {name}"),
            Refusal::NotInstalled(bundle) => write!(
                f,
                "the system does not know the app {bundle}: only an installed app can be the default"
            ),
            Refusal::Status(status) => write!(f, "Launch Services refused (code {status})"),
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::{CStr, CString, c_char, c_void};

    use super::Refusal;

    pub const SUPPORTED: bool = true;

    type CFStringRef = *const c_void;
    type CFAllocatorRef = *const c_void;
    type CFIndex = isize;
    type OSStatus = i32;

    const UTF8: u32 = 0x0800_0100;
    const ROLES_ALL: u32 = 0xFFFF_FFFF;
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

    struct CfString(CFStringRef);

    impl CfString {
        fn new(text: &str) -> Option<Self> {
            let text = CString::new(text).ok()?;
            let raw = unsafe { CFStringCreateWithCString(std::ptr::null(), text.as_ptr(), UTF8) };
            (!raw.is_null()).then_some(CfString(raw))
        }

        fn to_string(&self) -> Option<String> {
            let mut buffer = [0 as c_char; 512];
            let done = unsafe {
                CFStringGetCString(self.0, buffer.as_mut_ptr(), buffer.len() as CFIndex, UTF8)
            };
            if done == 0 {
                return None;
            }
            let text = unsafe { CStr::from_ptr(buffer.as_ptr()) };
            Some(text.to_string_lossy().into_owned())
        }
    }

    impl Drop for CfString {
        fn drop(&mut self) {
            unsafe { CFRelease(self.0) }
        }
    }

    pub fn default_handler(content_type: &str) -> Option<String> {
        let kind = CfString::new(content_type)?;
        let raw = unsafe { LSCopyDefaultRoleHandlerForContentType(kind.0, ROLES_ALL) };
        if raw.is_null() {
            return None;
        }
        CfString(raw).to_string()
    }

    pub fn set_default_handler(content_type: &str, bundle: &str) -> Result<(), Refusal> {
        let kind = CfString::new(content_type)
            .ok_or_else(|| Refusal::BadName(content_type.to_string()))?;
        let handler = CfString::new(bundle).ok_or_else(|| Refusal::BadName(bundle.to_string()))?;
        let status = unsafe { LSSetDefaultRoleHandlerForContentType(kind.0, ROLES_ALL, handler.0) };
        match status {
            0 => Ok(()),
            NOT_FOUND => Err(Refusal::NotInstalled(bundle.to_string())),
            other => Err(Refusal::Status(other)),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

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

#[cfg(not(target_os = "macos"))]
mod imp {
    pub const SUPPORTED: bool = false;

    pub fn default_handler(_content_type: &str) -> Option<String> {
        None
    }

    use super::Refusal;

    pub fn set_default_handler(_content_type: &str, _bundle: &str) -> Result<(), Refusal> {
        Err(Refusal::Unsupported)
    }
}
