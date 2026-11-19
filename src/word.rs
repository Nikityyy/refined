//! Word's default keyboard paste can merge heading styles into body text.

use std::{mem::size_of, ptr};
use windows::core::{w, Interface, GUID, PCWSTR};
use windows::Win32::Foundation::{DISP_E_PARAMNOTFOUND, HWND};
use windows::Win32::System::Com::{
    CoInitializeEx, CoUninitialize, IDispatch, COINIT_APARTMENTTHREADED, DISPATCH_FLAGS,
    DISPATCH_METHOD, DISPATCH_PROPERTYGET, DISPPARAMS,
};
use windows::Win32::System::Variant::{VariantClear, VARIANT, VT_DISPATCH, VT_ERROR, VT_I4};
use windows::Win32::UI::Accessibility::AccessibleObjectFromWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetClassNameW, GetForegroundWindow, GetGUIThreadInfo, GA_PARENT, GUITHREADINFO,
    OBJID_NATIVEOM,
};

/// Returns None when Word automation is unavailable, so the caller can use
/// normal clipboard paste. Once invoked, PasteSpecial is never retried.
pub fn paste_html(target: HWND) -> Option<bool> {
    if !has_class(target, "OpusApp") {
        return None;
    }
    let mut info = GUITHREADINFO {
        cbSize: size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    unsafe { GetGUIThreadInfo(0, &mut info) }.ok()?;
    let mut pane = info.hwndFocus;
    while !pane.0.is_null() && pane != target && !has_class(pane, "_WwG") {
        pane = unsafe { GetAncestor(pane, GA_PARENT) };
    }
    if !has_class(pane, "_WwG") {
        return None;
    }

    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
        .ok()
        .ok()?;
    let _com = ComApartment;
    unsafe {
        let mut raw = ptr::null_mut();
        AccessibleObjectFromWindow(pane, OBJID_NATIVEOM.0 as u32, &IDispatch::IID, &mut raw)
            .ok()?;
        if raw.is_null() {
            return None;
        }
        let window = IDispatch::from_raw(raw);
        let mut value = VARIANT::default();
        let got_selection = invoke(
            &window,
            w!("Selection"),
            DISPATCH_PROPERTYGET,
            &mut [],
            Some(&mut value),
        )
        .is_ok();
        let selection = {
            let body = &*value.Anonymous.Anonymous;
            if got_selection && body.vt == VT_DISPATCH {
                (&*body.Anonymous.pdispVal).clone()
            } else {
                None
            }
        };
        let _ = VariantClear(&mut value);
        let selection = selection?;
        if GetForegroundWindow() != target {
            return Some(false);
        }

        // IDispatch reverses positional arguments. DataType is the fifth
        // PasteSpecial argument; the earlier four are optional.
        let mut args: [VARIANT; 5] = std::array::from_fn(|_| {
            let mut arg = VARIANT::default();
            let body = &mut *arg.Anonymous.Anonymous;
            body.vt = VT_ERROR;
            body.Anonymous.scode = DISP_E_PARAMNOTFOUND.0;
            arg
        });
        let data_type = &mut *args[0].Anonymous.Anonymous;
        data_type.vt = VT_I4;
        data_type.Anonymous.lVal = 10; // wdPasteHTML
        Some(
            invoke(
                &selection,
                w!("PasteSpecial"),
                DISPATCH_METHOD,
                &mut args,
                None,
            )
            .is_ok(),
        )
    }
}

fn has_class(hwnd: HWND, name: &str) -> bool {
    let mut class = [0; 32];
    let length = unsafe { GetClassNameW(hwnd, &mut class) } as usize;
    class[..length].iter().copied().eq(name.encode_utf16())
}

unsafe fn invoke(
    dispatch: &IDispatch,
    name: PCWSTR,
    flags: DISPATCH_FLAGS,
    args: &mut [VARIANT],
    result: Option<*mut VARIANT>,
) -> windows::core::Result<()> {
    let mut id = 0;
    let iid = GUID::zeroed();
    dispatch.GetIDsOfNames(&iid, &name, 1, 0, &mut id)?;
    let params = DISPPARAMS {
        rgvarg: args.as_mut_ptr(),
        cArgs: args.len() as u32,
        ..Default::default()
    };
    dispatch.Invoke(id, &iid, 0, flags, &params, result, None, None)
}

struct ComApartment;

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() }
    }
}
