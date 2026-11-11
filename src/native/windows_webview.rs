use tauri::WebviewWindow;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
    ICoreWebView2_19,
};
use windows_core::Interface;

use crate::app::Backdrop;

pub(crate) fn apply_backdrop(
    window: &WebviewWindow,
    backdrop: Backdrop,
) -> Result<(), Box<dyn std::error::Error>> {
    use windows::Win32::{
        Graphics::Dwm::{
            DWM_SYSTEMBACKDROP_TYPE, DWMSBT_MAINWINDOW, DWMSBT_TABBEDWINDOW,
            DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE, DwmExtendFrameIntoClientArea,
            DwmGetWindowAttribute, DwmSetWindowAttribute,
        },
        UI::Controls::MARGINS,
    };

    let material = match backdrop {
        Backdrop::Mica => DWMSBT_MAINWINDOW,
        Backdrop::Tabbed => DWMSBT_TABBEDWINDOW,
        Backdrop::Acrylic => DWMSBT_TRANSIENTWINDOW,
        Backdrop::Opaque | Backdrop::Transparent => return Ok(()),
    };
    let hwnd = window.hwnd()?;
    let glass = MARGINS {
        cxLeftWidth: -1,
        cxRightWidth: -1,
        cyTopHeight: -1,
        cyBottomHeight: -1,
    };
    // The window is created on the event thread and both structures outlive these calls.
    unsafe {
        DwmExtendFrameIntoClientArea(hwnd, &glass)?;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            std::ptr::from_ref(&material).cast(),
            std::mem::size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32,
        )?;
        let mut applied = DWM_SYSTEMBACKDROP_TYPE::default();
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            std::ptr::from_mut(&mut applied).cast(),
            std::mem::size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32,
        )?;
        if applied != material {
            return Err(format!(
                "requested DWM material {}, Windows reported {}",
                material.0, applied.0
            )
            .into());
        }
    }
    Ok(())
}

pub(crate) fn set_backgrounded(window: &WebviewWindow, hidden: bool) {
    let _ = window.with_webview(move |webview| {
        let target = if hidden {
            COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
        } else {
            COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
        };
        // Tauri runs the callback on the webview's apartment thread.
        unsafe {
            let controller = webview.controller();
            let _ = controller.SetIsVisible(!hidden);
            let Ok(view) = controller.CoreWebView2() else {
                return;
            };
            if let Ok(memory) = view.cast::<ICoreWebView2_19>() {
                let _ = memory.SetMemoryUsageTargetLevel(target);
            }
        }
    });
}
