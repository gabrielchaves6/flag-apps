// flag-win: shared Windows utilities for the flag-apps family.
// All functions are unsafe-heavy Win32; caller guarantees single-threaded message-loop usage.
#![allow(static_mut_refs)]

use core::ffi::c_void;
use windows::core::PCWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::Urlmon::URLDownloadToFileW;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// ──────────────────────────── string helpers ────────────────────────────────

pub fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn wn(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

pub fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF((r as u32) | ((g as u32) << 8) | ((b as u32) << 16))
}

pub fn in_rect(r: RECT, x: i32, y: i32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

// ──────────────────────────── GDI icons ─────────────────────────────────────

pub unsafe fn make_font(height: i32, weight: i32, underline: bool) -> HFONT {
    CreateFontW(
        -height, 0, 0, 0, weight, 0,
        if underline { 1 } else { 0 }, 0,
        DEFAULT_CHARSET.0 as u32,
        OUT_DEFAULT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32,
        (DEFAULT_PITCH.0 | (FF_DONTCARE.0 << 4) as u8) as u32,
        PCWSTR(w("Segoe UI").as_ptr()),
    )
}

pub unsafe fn make_text_icon(bg: COLORREF, label: &str) -> HICON {
    let sz = 32i32;
    let screen_dc = GetDC(None);
    let mem_dc = CreateCompatibleDC(screen_dc);
    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: core::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: sz,
            biHeight: -sz,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut c_void = core::ptr::null_mut();
    let dib = CreateDIBSection(mem_dc, &mut bmi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap_or_default();
    let old = SelectObject(mem_dc, dib);

    let full = RECT { left: 0, top: 0, right: sz, bottom: sz };
    let brush = CreateSolidBrush(bg);
    FillRect(mem_dc, &full, brush);
    let _ = DeleteObject(brush);

    let pt = if label.chars().count() >= 2 { 18 } else { 22 };
    let font = make_font(pt, 700, false);
    let old_font = SelectObject(mem_dc, font);
    SetBkMode(mem_dc, TRANSPARENT);
    let _ = SetTextColor(mem_dc, rgb(255, 255, 255));
    let mut txt = wn(label);
    let mut tr = full;
    DrawTextW(mem_dc, &mut txt, &mut tr, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
    SelectObject(mem_dc, old_font);
    let _ = DeleteObject(font);

    let p = bits as *mut u8;
    for i in 0..(sz * sz) as usize {
        *p.add(i * 4 + 3) = 255;
    }

    SelectObject(mem_dc, old);
    let mask_bits = vec![0u8; (sz * sz) as usize];
    let mask = CreateBitmap(sz, sz, 1, 1, Some(mask_bits.as_ptr() as *const _));
    let ii = ICONINFO { fIcon: TRUE, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: dib };
    let hicon = CreateIconIndirect(&ii).unwrap_or_default();
    let _ = DeleteObject(mask);
    let _ = DeleteObject(dib);
    let _ = DeleteDC(mem_dc);
    ReleaseDC(None, screen_dc);
    hicon
}

pub unsafe fn make_dots_icon(bg: COLORREF) -> HICON {
    let sz = 32i32;
    let screen_dc = GetDC(None);
    let mem_dc = CreateCompatibleDC(screen_dc);
    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: core::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: sz, biHeight: -sz, biPlanes: 1, biBitCount: 32,
            biCompression: BI_RGB.0, ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut c_void = core::ptr::null_mut();
    let dib = CreateDIBSection(mem_dc, &mut bmi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap_or_default();
    let old = SelectObject(mem_dc, dib);
    let full = RECT { left: 0, top: 0, right: sz, bottom: sz };
    let brush = CreateSolidBrush(bg);
    FillRect(mem_dc, &full, brush);
    let _ = DeleteObject(brush);
    let white = CreateSolidBrush(rgb(255, 255, 255));
    let null_pen = GetStockObject(NULL_PEN);
    let old_b = SelectObject(mem_dc, white);
    let old_p = SelectObject(mem_dc, null_pen);
    let r = 3;
    for cx in [9i32, 16, 23] {
        let _ = Ellipse(mem_dc, cx - r, 16 - r, cx + r + 1, 16 + r + 1);
    }
    SelectObject(mem_dc, old_b);
    SelectObject(mem_dc, old_p);
    let _ = DeleteObject(white);
    let p = bits as *mut u8;
    for i in 0..(sz * sz) as usize { *p.add(i * 4 + 3) = 255; }
    SelectObject(mem_dc, old);
    let mask_bits = vec![0u8; (sz * sz) as usize];
    let mask = CreateBitmap(sz, sz, 1, 1, Some(mask_bits.as_ptr() as *const _));
    let ii = ICONINFO { fIcon: TRUE, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: dib };
    let hicon = CreateIconIndirect(&ii).unwrap_or_default();
    let _ = DeleteObject(mask); let _ = DeleteObject(dib); let _ = DeleteDC(mem_dc);
    ReleaseDC(None, screen_dc);
    hicon
}

// ──────────────────────────── window chrome ─────────────────────────────────

pub const CLOSE_BTN_W: i32 = 46;
pub const CLOSE_BTN_H: i32 = 36;
pub const DRAG_STRIP_H: i32 = 92;

pub fn close_btn_rect(client_right: i32) -> RECT {
    RECT { left: client_right - CLOSE_BTN_W, top: 0, right: client_right, bottom: CLOSE_BTN_H }
}

pub unsafe fn paint_close_btn(hdc: HDC, client_right: i32, hot: bool) -> RECT {
    let rc = close_btn_rect(client_right);
    if hot {
        let hb = CreateSolidBrush(rgb(196, 43, 43));
        FillRect(hdc, &rc, hb);
        let _ = DeleteObject(hb);
    }
    let cx = (rc.left + rc.right) / 2;
    let cy = (rc.top + rc.bottom) / 2;
    let s = 5;
    let pen = CreatePen(PS_SOLID, 1, if hot { rgb(255, 255, 255) } else { rgb(180, 186, 198) });
    let old = SelectObject(hdc, pen);
    let _ = MoveToEx(hdc, cx - s, cy - s, None);
    let _ = LineTo(hdc, cx + s + 1, cy + s + 1);
    let _ = MoveToEx(hdc, cx + s, cy - s, None);
    let _ = LineTo(hdc, cx - s - 1, cy + s + 1);
    SelectObject(hdc, old);
    let _ = DeleteObject(pen);
    rc
}

pub unsafe fn setup_chrome(hwnd: HWND, icon: isize) {
    let _ = SendMessageW(hwnd, WM_SETICON, WPARAM(0), LPARAM(icon));
    let _ = SendMessageW(hwnd, WM_SETICON, WPARAM(1), LPARAM(icon));
    let round: i32 = 2;
    let _ = DwmSetWindowAttribute(hwnd, DWMWINDOWATTRIBUTE(33), &round as *const _ as *const _, 4);
}

pub unsafe fn begin_drag_if_top(hwnd: HWND, x: i32, y: i32, client_right: i32) -> bool {
    if y < DRAG_STRIP_H && !in_rect(close_btn_rect(client_right), x, y) {
        let _ = ReleaseCapture();
        SendMessageW(hwnd, WM_NCLBUTTONDOWN, WPARAM(HTCAPTION as usize), LPARAM(0));
        return true;
    }
    false
}

pub unsafe fn get_work_area() -> RECT {
    let mut work = RECT::default();
    let _ = SystemParametersInfoW(
        SPI_GETWORKAREA, 0,
        Some(&mut work as *mut _ as *mut c_void),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    );
    work
}

// ──────────────────────────── registry helpers ──────────────────────────────

pub unsafe fn reg_read_str(subkey: &str, value: &str) -> Option<String> {
    let sk = w(subkey);
    let v = w(value);
    let mut size: u32 = 0;
    let rc = RegGetValueW(HKEY_CURRENT_USER, PCWSTR(sk.as_ptr()), PCWSTR(v.as_ptr()),
        RRF_RT_REG_SZ, None, None, Some(&mut size));
    if rc != ERROR_SUCCESS || size == 0 { return None; }
    let mut buf = vec![0u16; (size as usize) / 2];
    let rc = RegGetValueW(HKEY_CURRENT_USER, PCWSTR(sk.as_ptr()), PCWSTR(v.as_ptr()),
        RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size));
    if rc != ERROR_SUCCESS { return None; }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

pub unsafe fn reg_write_str(subkey: &str, value: &str, data: &str) {
    let sk = w(subkey);
    let mut hkey = HKEY::default();
    if RegCreateKeyExW(HKEY_CURRENT_USER, PCWSTR(sk.as_ptr()), 0, PCWSTR::null(),
        REG_OPTION_NON_VOLATILE, KEY_SET_VALUE, None, &mut hkey, None) != ERROR_SUCCESS { return; }
    let d = w(data);
    let bytes = core::slice::from_raw_parts(d.as_ptr() as *const u8, d.len() * 2);
    let _ = RegSetValueExW(hkey, PCWSTR(w(value).as_ptr()), 0, REG_SZ, Some(bytes));
    let _ = RegCloseKey(hkey);
}

pub unsafe fn reg_read_dword(subkey: &str, value: &str) -> Option<u32> {
    let sk = w(subkey);
    let v = w(value);
    let mut data: u32 = 0;
    let mut size = 4u32;
    let rc = RegGetValueW(HKEY_CURRENT_USER, PCWSTR(sk.as_ptr()), PCWSTR(v.as_ptr()),
        RRF_RT_REG_DWORD, None, Some(&mut data as *mut _ as *mut _), Some(&mut size));
    if rc == ERROR_SUCCESS { Some(data) } else { None }
}

pub unsafe fn reg_write_dword(subkey: &str, value: &str, data: u32) -> bool {
    let sk = w(subkey);
    let mut hkey = HKEY::default();
    if RegCreateKeyExW(HKEY_CURRENT_USER, PCWSTR(sk.as_ptr()), 0, PCWSTR::null(),
        REG_OPTION_NON_VOLATILE, KEY_SET_VALUE, None, &mut hkey, None) != ERROR_SUCCESS { return false; }
    let bytes = data.to_ne_bytes();
    let rc = RegSetValueExW(hkey, PCWSTR(w(value).as_ptr()), 0, REG_DWORD, Some(&bytes));
    let _ = RegCloseKey(hkey);
    rc == ERROR_SUCCESS
}

// ──────────────────────────── auto-update utilities ─────────────────────────

pub fn ver_tuple(s: &str) -> (u32, u32, u32) {
    let s = s.trim().trim_start_matches('v');
    let mut p = s.split(|c| c == '.' || c == '-' || c == '+');
    let n = |o: Option<&str>| o.and_then(|x| x.trim().parse::<u32>().ok()).unwrap_or(0);
    (n(p.next()), n(p.next()), n(p.next()))
}

pub fn json_string(body: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\"");
    let after = &body[body.find(&pat)? + pat.len()..];
    let after = &after[after.find(':')? + 1..];
    let after = &after[after.find('"')? + 1..];
    Some(after[..after.find('"')?].to_string())
}

pub fn find_exe_asset(body: &str) -> Option<String> {
    let key = "\"browser_download_url\"";
    let mut start = 0;
    while let Some(i) = body[start..].find(key) {
        let abs = start + i;
        if let Some(u) = json_string(&body[abs..], "browser_download_url") {
            if u.to_lowercase().ends_with(".exe") { return Some(u); }
        }
        start = abs + key.len();
    }
    None
}

pub unsafe fn url_download(url: &str, dest: &std::path::Path) -> bool {
    let url_w = w(url);
    let dest_w = w(&dest.to_string_lossy());
    URLDownloadToFileW(None, PCWSTR(url_w.as_ptr()), PCWSTR(dest_w.as_ptr()), 0, None).is_ok()
}

// ──────────────────────────── styled dialog (static-mut; single-threaded) ───

static mut DLG_ICON: isize = 0;
static mut DLG_CLOSE: RECT = RECT { left: 0, top: 0, right: 0, bottom: 0 };
static mut DLG_CLOSE_HOT: bool = false;
static mut DLG_HEADING: String = String::new();
static mut DLG_BODY: String = String::new();
static mut DLG_PRIMARY: String = String::new();
static mut DLG_SECONDARY: String = String::new();
static mut DLG_BTN_PRIMARY: RECT = RECT { left: 0, top: 0, right: 0, bottom: 0 };
static mut DLG_BTN_SECONDARY: RECT = RECT { left: 0, top: 0, right: 0, bottom: 0 };
static mut DLG_RESULT: i32 = 0;
static mut DLG_CLASS_REGISTERED: bool = false;
static mut DLG_WORK: RECT = RECT { left: 0, top: 0, right: 0, bottom: 0 };

const DLG_W: i32 = 430;
const DLG_H: i32 = 248;
const DLG_BTN_W: i32 = 116;
const DLG_BTN_H: i32 = 34;
const DLG_BTN_PAD_X: i32 = 18;
const DLG_BODY_GAP: i32 = 28;

unsafe fn button_width(hdc: HDC, label: &str) -> i32 {
    let font = make_font(17, 600, false);
    let of = SelectObject(hdc, font);
    let mut t = wn(label);
    let mut r = RECT::default();
    DrawTextW(hdc, &mut t, &mut r, DT_CALCRECT | DT_SINGLELINE);
    SelectObject(hdc, of);
    let _ = DeleteObject(font);
    (r.right - r.left + 2 * DLG_BTN_PAD_X).max(DLG_BTN_W)
}

unsafe fn paint_button(hdc: HDC, x: i32, y: i32, width: i32, label: &str, accent: bool) -> RECT {
    let rc = RECT { left: x, top: y, right: x + width, bottom: y + DLG_BTN_H };
    let fill = CreateSolidBrush(if accent { rgb(56, 118, 240) } else { rgb(48, 52, 62) });
    let pen  = CreatePen(PS_SOLID, 1, if accent { rgb(56, 118, 240) } else { rgb(74, 80, 92) });
    let old_b = SelectObject(hdc, fill);
    let old_p = SelectObject(hdc, pen);
    let _ = RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, 12, 12);
    SelectObject(hdc, old_b); SelectObject(hdc, old_p);
    let _ = DeleteObject(fill); let _ = DeleteObject(pen);
    let font = make_font(17, 600, false);
    let of = SelectObject(hdc, font);
    SetBkMode(hdc, TRANSPARENT);
    let _ = SetTextColor(hdc, if accent { rgb(255, 255, 255) } else { rgb(214, 219, 228) });
    let mut t = wn(label);
    let mut r = rc;
    DrawTextW(hdc, &mut t, &mut r, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
    SelectObject(hdc, of); let _ = DeleteObject(font);
    rc
}

extern "system" fn dlg_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                let bg = CreateSolidBrush(rgb(24, 27, 35));
                FillRect(hdc, &rc, bg);
                let _ = DeleteObject(bg);
                let pad = 28;
                if DLG_ICON != 0 {
                    let _ = DrawIconEx(hdc, pad, 30, HICON(DLG_ICON as *mut _), 48, 48, 0, None, DI_NORMAL);
                }
                SetBkMode(hdc, TRANSPARENT);
                let title_font = make_font(24, 600, false);
                let body_font  = make_font(17, 400, false);
                let text_x = pad + 48 + 16;
                let old = SelectObject(hdc, title_font);
                let _ = SetTextColor(hdc, rgb(250, 250, 252));
                let mut r = RECT { left: text_x, top: 38, right: rc.right - pad, bottom: 74 };
                let mut t = wn(&DLG_HEADING);
                DrawTextW(hdc, &mut t, &mut r, DT_LEFT | DT_SINGLELINE);
                SelectObject(hdc, body_font);
                let _ = SetTextColor(hdc, rgb(196, 202, 214));
                let mut r2 = RECT { left: pad, top: 96, right: rc.right - pad, bottom: rc.bottom - 64 };
                let mut t2 = wn(&DLG_BODY);
                DrawTextW(hdc, &mut t2, &mut r2, DT_LEFT | DT_WORDBREAK);
                SelectObject(hdc, old);
                let _ = DeleteObject(title_font); let _ = DeleteObject(body_font);
                let by = rc.bottom - 20 - DLG_BTN_H;
                let pw = button_width(hdc, &DLG_PRIMARY);
                let px = rc.right - pad - pw;
                DLG_BTN_PRIMARY = paint_button(hdc, px, by, pw, &DLG_PRIMARY, true);
                if !DLG_SECONDARY.is_empty() {
                    let sw = button_width(hdc, &DLG_SECONDARY);
                    DLG_BTN_SECONDARY = paint_button(hdc, px - 12 - sw, by, sw, &DLG_SECONDARY, false);
                } else {
                    DLG_BTN_SECONDARY = RECT::default();
                }
                DLG_CLOSE = paint_close_btn(hdc, rc.right, DLG_CLOSE_HOT);
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let x = (lparam.0 & 0xffff) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                begin_drag_if_top(hwnd, x, y, rc.right);
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let x = (lparam.0 & 0xffff) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
                if in_rect(DLG_CLOSE, x, y)           { DLG_RESULT = 2; let _ = DestroyWindow(hwnd); }
                else if in_rect(DLG_BTN_PRIMARY, x, y) { DLG_RESULT = 1; let _ = DestroyWindow(hwnd); }
                else if !DLG_SECONDARY.is_empty() && in_rect(DLG_BTN_SECONDARY, x, y) {
                    DLG_RESULT = 2; let _ = DestroyWindow(hwnd);
                }
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                let x = (lparam.0 & 0xffff) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
                let hot = in_rect(DLG_CLOSE, x, y);
                if hot != DLG_CLOSE_HOT {
                    DLG_CLOSE_HOT = hot;
                    let _ = InvalidateRect(hwnd, Some(&DLG_CLOSE), FALSE);
                }
                LRESULT(0)
            }
            WM_SETCURSOR => {
                let mut pt = POINT::default();
                let _ = GetCursorPos(&mut pt);
                let _ = ScreenToClient(hwnd, &mut pt);
                if in_rect(DLG_BTN_PRIMARY, pt.x, pt.y) || in_rect(DLG_CLOSE, pt.x, pt.y)
                    || (!DLG_SECONDARY.is_empty() && in_rect(DLG_BTN_SECONDARY, pt.x, pt.y))
                {
                    if let Ok(hand) = LoadCursorW(None, IDC_HAND) { SetCursor(hand); }
                    return LRESULT(1);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_KEYDOWN => {
                match windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(wparam.0 as u16) {
                    k if k == VK_ESCAPE => { DLG_RESULT = 2; let _ = DestroyWindow(hwnd); }
                    _ => {
                        // VK_RETURN = 0x0D
                        if wparam.0 == 0x0D { DLG_RESULT = 1; let _ = DestroyWindow(hwnd); }
                    }
                }
                LRESULT(0)
            }
            WM_GETICON => LRESULT(DLG_ICON),
            WM_CLOSE => {
                if DLG_RESULT == 0 { DLG_RESULT = 2; }
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

pub unsafe fn show_dialog(icon: isize, work: RECT, heading: &str, body: &str, primary: &str, secondary: &str) -> bool {
    let hinstance: HINSTANCE = GetModuleHandleW(None).map(|h| h.into()).unwrap_or_default();
    let class = w("FlagAppsDialog");
    if !DLG_CLASS_REGISTERED {
        let wc = WNDCLASSW {
            lpfnWndProc: Some(dlg_wndproc),
            hInstance: hinstance,
            lpszClassName: PCWSTR(class.as_ptr()),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hIcon: HICON(icon as *mut _),
            ..Default::default()
        };
        RegisterClassW(&wc);
        DLG_CLASS_REGISTERED = true;
    }
    DLG_ICON = icon;
    DLG_WORK = work;
    DLG_HEADING  = heading.to_string();
    DLG_BODY     = body.to_string();
    DLG_PRIMARY  = primary.to_string();
    DLG_SECONDARY = secondary.to_string();
    DLG_RESULT = 0;

    let dlg_h = {
        let screen = GetDC(None);
        let font = make_font(17, 400, false);
        let of = SelectObject(screen, font);
        let mut t = wn(body);
        let mut r = RECT { left: 0, top: 0, right: DLG_W - 56, bottom: 0 };
        DrawTextW(screen, &mut t, &mut r, DT_LEFT | DT_WORDBREAK | DT_CALCRECT);
        let body_h = r.bottom - r.top;
        SelectObject(screen, of); let _ = DeleteObject(font);
        ReleaseDC(None, screen);
        (96 + body_h + DLG_BODY_GAP + DLG_BTN_H + 20).max(DLG_H)
    };

    let (cx, cy) = if work.right > work.left {
        (work.left + (work.right - work.left) / 2, work.top + (work.bottom - work.top) / 2)
    } else {
        (GetSystemMetrics(SM_CXSCREEN) / 2, GetSystemMetrics(SM_CYSCREEN) / 2)
    };

    let hwnd = match CreateWindowExW(WINDOW_EX_STYLE(0), PCWSTR(class.as_ptr()),
        PCWSTR(w("FlagApps").as_ptr()), WS_POPUP,
        cx - DLG_W / 2, cy - dlg_h / 2, DLG_W, dlg_h, None, None, hinstance, None) {
        Ok(h) => h,
        Err(_) => return false,
    };
    setup_chrome(hwnd, icon);
    DLG_CLOSE_HOT = false;
    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);

    let mut m = MSG::default();
    loop {
        if DLG_RESULT != 0 { break; }
        if !GetMessageW(&mut m, None, 0, 0).as_bool() { break; }
        let _ = TranslateMessage(&m);
        DispatchMessageW(&m);
    }
    if IsWindow(hwnd).as_bool() { let _ = DestroyWindow(hwnd); }
    DLG_RESULT == 1
}

// ──────────────────────────── About window ──────────────────────────────────

static mut ABOUT_ICON: isize = 0;
static mut ABOUT_LINK: RECT = RECT { left: 0, top: 0, right: 0, bottom: 0 };
static mut ABOUT_CLOSE: RECT = RECT { left: 0, top: 0, right: 0, bottom: 0 };
static mut ABOUT_CLOSE_HOT: bool = false;

const ABOUT_URL: &str = "https://github.com/gabrielchaves6/flag-apps";

extern "system" fn about_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                let bg = CreateSolidBrush(rgb(24, 27, 35));
                FillRect(hdc, &rc, bg);
                let _ = DeleteObject(bg);

                let pad = 28;
                if ABOUT_ICON != 0 {
                    let _ = DrawIconEx(hdc, pad, 34, HICON(ABOUT_ICON as *mut _), 56, 56, 0, None, DI_NORMAL);
                }

                SetBkMode(hdc, TRANSPARENT);
                let title_font = make_font(27, 600, false);
                let body_font  = make_font(17, 400, false);
                let link_font  = make_font(17, 400, true);
                let small_font = make_font(14, 400, false);

                let text_x = pad + 56 + 18;
                let old = SelectObject(hdc, title_font);
                let _ = SetTextColor(hdc, rgb(250, 250, 252));
                let mut r = RECT { left: text_x, top: 36, right: rc.right - pad, bottom: 68 };
                let mut t = wn("FlagApps");
                DrawTextW(hdc, &mut t, &mut r, DT_LEFT | DT_SINGLELINE);

                SelectObject(hdc, body_font);
                let _ = SetTextColor(hdc, rgb(150, 156, 170));
                let mut r2 = RECT { left: text_x, top: 70, right: rc.right - pad, bottom: 94 };
                let mut t2 = wn(&format!("Version {}", env!("CARGO_PKG_VERSION")));
                DrawTextW(hdc, &mut t2, &mut r2, DT_LEFT | DT_SINGLELINE);

                let _ = SetTextColor(hdc, rgb(196, 202, 214));
                let mut r3 = RECT { left: pad, top: 116, right: rc.right - pad, bottom: 144 };
                let mut t3 = wn("Unified tray utilities for Windows");
                DrawTextW(hdc, &mut t3, &mut r3, DT_LEFT | DT_SINGLELINE);

                SelectObject(hdc, small_font);
                let _ = SetTextColor(hdc, rgb(120, 126, 140));
                let mut r4 = RECT { left: pad, top: 146, right: rc.right - pad, bottom: 174 };
                let mut t4 = wn("AnyFlag · KeyFlag · EnergyFlag · NewTrayFlag · ClipFlag · DeskFlag");
                DrawTextW(hdc, &mut t4, &mut r4, DT_LEFT | DT_SINGLELINE);

                SelectObject(hdc, link_font);
                let _ = SetTextColor(hdc, rgb(90, 150, 245));
                let link_top = rc.bottom - 40;
                let mut r5 = RECT { left: pad, top: link_top, right: rc.right - pad, bottom: rc.bottom - 14 };
                let mut t5 = wn("github.com/gabrielchaves6/flag-apps");
                DrawTextW(hdc, &mut t5, &mut r5, DT_LEFT | DT_SINGLELINE);
                let mut sz = SIZE::default();
                let _ = GetTextExtentPoint32W(hdc, &t5, &mut sz);
                ABOUT_LINK = RECT { left: pad, top: link_top, right: pad + sz.cx, bottom: link_top + sz.cy };

                SelectObject(hdc, old);
                let _ = DeleteObject(title_font); let _ = DeleteObject(body_font);
                let _ = DeleteObject(link_font); let _ = DeleteObject(small_font);
                ABOUT_CLOSE = paint_close_btn(hdc, rc.right, ABOUT_CLOSE_HOT);
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let x = (lparam.0 & 0xffff) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                begin_drag_if_top(hwnd, x, y, rc.right);
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let x = (lparam.0 & 0xffff) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
                if in_rect(ABOUT_CLOSE, x, y) {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                } else if in_rect(ABOUT_LINK, x, y) {
                    let _ = ShellExecuteW(None, PCWSTR(w("open").as_ptr()),
                        PCWSTR(w(ABOUT_URL).as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
                }
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                let x = (lparam.0 & 0xffff) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
                let hot = in_rect(ABOUT_CLOSE, x, y);
                if hot != ABOUT_CLOSE_HOT {
                    ABOUT_CLOSE_HOT = hot;
                    let _ = InvalidateRect(hwnd, Some(&ABOUT_CLOSE), FALSE);
                }
                LRESULT(0)
            }
            WM_SETCURSOR => {
                let mut pt = POINT::default();
                let _ = GetCursorPos(&mut pt);
                let _ = ScreenToClient(hwnd, &mut pt);
                if in_rect(ABOUT_LINK, pt.x, pt.y) || in_rect(ABOUT_CLOSE, pt.x, pt.y) {
                    if let Ok(hand) = LoadCursorW(None, IDC_HAND) { SetCursor(hand); }
                    return LRESULT(1);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_KEYDOWN if windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(wparam.0 as u16) == VK_ESCAPE => {
                let _ = ShowWindow(hwnd, SW_HIDE);
                LRESULT(0)
            }
            WM_GETICON => LRESULT(ABOUT_ICON),
            WM_CLOSE => { let _ = ShowWindow(hwnd, SW_HIDE); LRESULT(0) }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

pub unsafe fn create_about_window(hinstance: HINSTANCE, icon: isize) -> windows::core::Result<HWND> {
    ABOUT_ICON = icon;
    let class = w("FlagAppsAbout");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(about_wndproc),
        hInstance: hinstance,
        lpszClassName: PCWSTR(class.as_ptr()),
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        hIcon: HICON(icon as *mut _),
        ..Default::default()
    };
    RegisterClassW(&wc);
    let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), PCWSTR(class.as_ptr()),
        PCWSTR(w("About FlagApps").as_ptr()), WS_POPUP,
        0, 0, 480, 280, None, None, hinstance, None)?;
    setup_chrome(hwnd, icon);
    Ok(hwnd)
}

pub unsafe fn show_about(hwnd: HWND, icon: isize, work: RECT) {
    ABOUT_ICON = icon;
    let (w_px, h_px) = (480, 280);
    let (cx, cy) = if work.right > work.left {
        (work.left + (work.right - work.left) / 2, work.top + (work.bottom - work.top) / 2)
    } else {
        (GetSystemMetrics(SM_CXSCREEN) / 2, GetSystemMetrics(SM_CYSCREEN) / 2)
    };
    let _ = SetWindowPos(hwnd, HWND_TOPMOST, cx - w_px / 2, cy - h_px / 2, w_px, h_px, SWP_SHOWWINDOW);
    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);
    let _ = InvalidateRect(hwnd, None, TRUE);
}
