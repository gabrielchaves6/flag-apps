#![windows_subsystem = "windows"]
#![allow(static_mut_refs)]

mod modules;

fn install_panic_log() {
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("{info}");
        let path = std::env::temp_dir().join("flagapp_panic.txt");
        let _ = std::fs::write(&path, &msg);
    }));
}

use modules::{
    ids::{GLOBAL_ABOUT, GLOBAL_UPDATE, GLOBAL_EXIT},
    anyflag::AnyFlag, keyflag::KeyFlag, energyflag::EnergyFlag,
    newtrayflag::NewTrayFlag, clipflag::ClipFlag, deskflag::DeskFlag,
    FlagModule,
};
use flag_win::{create_about_window, get_work_area, make_text_icon, rgb, show_about, show_dialog, w};
use windows::core::PCWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const WM_TRAY: u32 = WM_APP + 1;
const WM_SYNC_ICON: u32 = WM_APP + 2; // any module posts this to request a tray icon refresh

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const RELEASES_API: &str = "https://api.github.com/repos/gabrielchaves6/flag-apps/releases/latest";
const MSG_CLASS: &str = "FlagAppsMsg";

static mut G_ABOUT_HWND: Option<HWND> = None;
static mut G_WORK: RECT = RECT { left: 0, top: 0, right: 0, bottom: 0 };
static mut G_NID: NOTIFYICONDATAW = unsafe { core::mem::zeroed() };
static mut G_ICON_MAIN: HICON = HICON(core::ptr::null_mut());

unsafe fn current_tray_icon(modules: &[Box<dyn FlagModule>]) -> HICON {
    // Priority: AnyFlag flashing > main brand icon
    // (AnyFlag is index 0; it exposes flash state via command 0 side-channel)
    // Simple approach: always use static brand icon for tray; modules show state via menu items.
    G_ICON_MAIN
}

unsafe fn update_tray_icon(modules: &[Box<dyn FlagModule>]) {
    let icon = current_tray_icon(modules);
    G_NID.hIcon = icon;
    G_NID.uFlags |= NIF_ICON;
    let _ = Shell_NotifyIconW(NIM_MODIFY, &G_NID);
}

unsafe fn build_modules_menu(modules: &mut Vec<Box<dyn FlagModule>>) -> HMENU {
    let hmenu = CreatePopupMenu().unwrap_or_default();
    for (i, module) in modules.iter().enumerate() {
        if i > 0 { let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null()); }
        let sep_label = w(&format!("── {} ──", module.name()));
        let _ = AppendMenuW(hmenu, MF_STRING | MF_GRAYED, 0, PCWSTR(sep_label.as_ptr()));
        module.append_menu(hmenu);
    }
    hmenu
}

unsafe fn build_global_menu() -> HMENU {
    let hmenu = CreatePopupMenu().unwrap_or_default();
    let _ = AppendMenuW(hmenu, MF_STRING, GLOBAL_ABOUT as usize,  PCWSTR(w("About FlagApps").as_ptr()));
    let _ = AppendMenuW(hmenu, MF_STRING, GLOBAL_UPDATE as usize, PCWSTR(w("Check for updates").as_ptr()));
    let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
    let _ = AppendMenuW(hmenu, MF_STRING, GLOBAL_EXIT as usize,   PCWSTR(w("Exit").as_ptr()));
    hmenu
}

unsafe fn check_for_updates(hwnd: HWND) {
    let hwnd_raw = hwnd.0 as isize;
    std::thread::spawn(move || {
        let hwnd = HWND(hwnd_raw as *mut _);
        let body = http_get(RELEASES_API);
        if body.is_empty() {
            let _ = PostMessageW(hwnd, WM_APP + 30, WPARAM(0), LPARAM(0));
            return;
        }
        let remote = flag_win::json_string(&body, "tag_name").unwrap_or_default();
        let r = flag_win::ver_tuple(&remote);
        let l = flag_win::ver_tuple(APP_VERSION);
        if r > l {
            let url = flag_win::find_exe_asset(&body).unwrap_or_default();
            UPDATE_URL_BUF.lock().map(|mut g| *g = url).ok();
            let _ = PostMessageW(hwnd, WM_APP + 31, WPARAM(0), LPARAM(0));
        } else {
            let _ = PostMessageW(hwnd, WM_APP + 32, WPARAM(0), LPARAM(0));
        }
    });
}

use std::sync::Mutex;
static UPDATE_URL_BUF: Mutex<String> = Mutex::new(String::new());

fn http_get(url: &str) -> String {
    // Use WinHTTP via shell/urlmon — no external deps
    // Simple fallback: write to temp file, read back
    let tmp = std::env::temp_dir().join("flagapp_update.json");
    unsafe { if flag_win::url_download(url, &tmp) { std::fs::read_to_string(&tmp).unwrap_or_default() } else { String::new() } }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let modules_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Vec<Box<dyn FlagModule>>;
        let modules = if !modules_ptr.is_null() { &mut *modules_ptr } else { return DefWindowProcW(hwnd, msg, wparam, lparam); };

        // Dispatch to modules first
        for m in modules.iter_mut() {
            if let Some(r) = m.on_message(hwnd, msg, wparam, lparam) { return r; }
        }

        match msg {
            WM_TRAY => {
                let event = (lparam.0 & 0xffff) as u32;
                match event {
                    // Left click — module toggles
                    0x0200 | 0x0201 | 0x0202 | 0x0400 | 0x0401 => {
                        // NIN_SELECT=0x0400, WM_LBUTTONUP=0x0202, WM_LBUTTONDOWN=0x0201
                        if event == 0x0202 || event == 0x0400 {
                            G_WORK = get_work_area();
                            let hmenu = build_modules_menu(modules);
                            let mut pt = POINT::default();
                            GetCursorPos(&mut pt);
                            SetForegroundWindow(hwnd);
                            TrackPopupMenu(hmenu, TPM_BOTTOMALIGN | TPM_RIGHTALIGN, pt.x, pt.y, 0, hwnd, None);
                            PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
                            DestroyMenu(hmenu);
                        }
                    }
                    // Right click — global options
                    WM_RBUTTONUP | WM_CONTEXTMENU => {
                        G_WORK = get_work_area();
                        let hmenu = build_global_menu();
                        let mut pt = POINT::default();
                        GetCursorPos(&mut pt);
                        SetForegroundWindow(hwnd);
                        TrackPopupMenu(hmenu, TPM_BOTTOMALIGN | TPM_RIGHTALIGN, pt.x, pt.y, 0, hwnd, None);
                        PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
                        DestroyMenu(hmenu);
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_COMMAND => {
                let cmd = (wparam.0 & 0xffff) as u32;
                match cmd {
                    GLOBAL_ABOUT => {
                        if let Some(about) = G_ABOUT_HWND {
                            show_about(about, G_ICON_MAIN.0 as isize, G_WORK);
                        }
                    }
                    GLOBAL_UPDATE  => { check_for_updates(hwnd); }
                    GLOBAL_EXIT    => { let _ = DestroyWindow(hwnd); }
                    _ => {
                        for m in modules.iter_mut() {
                            if m.on_command(hwnd, cmd) { update_tray_icon(modules); break; }
                        }
                    }
                }
                LRESULT(0)
            }
            WM_TIMER => {
                let id = wparam.0;
                for m in modules.iter_mut() { m.on_timer(hwnd, id); }
                update_tray_icon(modules);
                LRESULT(0)
            }
            WM_HOTKEY => {
                let id = wparam.0 as i32;
                for m in modules.iter_mut() { m.on_hotkey(hwnd, id); }
                update_tray_icon(modules);
                LRESULT(0)
            }
            WM_CLIPBOARDUPDATE => {
                for m in modules.iter_mut() { m.on_clipboard_update(hwnd); }
                LRESULT(0)
            }
            WM_DISPLAYCHANGE => {
                for m in modules.iter_mut() { m.on_display_change(hwnd); }
                LRESULT(0)
            }
            WM_SYNC_ICON => {
                update_tray_icon(modules);
                LRESULT(0)
            }
            // Check-for-updates results
            m if m == WM_APP + 30 => { LRESULT(0) } // no internet — silently ignore
            m if m == WM_APP + 31 => {
                // Update available
                let url = UPDATE_URL_BUF.lock().map(|g| g.clone()).unwrap_or_default();
                let latest_ver = {
                    let tmp = std::env::temp_dir().join("flagapp_update.json");
                    let body = std::fs::read_to_string(&tmp).unwrap_or_default();
                    flag_win::json_string(&body, "tag_name").unwrap_or_else(|| "newer".into())
                };
                let body = format!("Version {} is available.\nYou have {}.\n\nDownload and replace now?", latest_ver, APP_VERSION);
                let ok = show_dialog(G_ICON_MAIN.0 as isize, G_WORK, "Update available", &body, "Update", "Later");
                if ok && !url.is_empty() {
                    let dest = std::env::current_exe().unwrap_or_default();
                    let tmp  = dest.with_extension("exe.new");
                    if flag_win::url_download(&url, &tmp) {
                        // Rename current → .old, new → current, relaunch
                        let old = dest.with_extension("exe.old");
                        let _ = std::fs::rename(&dest, &old);
                        if std::fs::rename(&tmp, &dest).is_ok() {
                            let _ = std::process::Command::new(&dest).spawn();
                            DestroyWindow(hwnd);
                        }
                    }
                }
                LRESULT(0)
            }
            m if m == WM_APP + 32 => {
                show_dialog(G_ICON_MAIN.0 as isize, G_WORK, "FlagApps", "You are on the latest version.", "OK", "");
                LRESULT(0)
            }
            WM_DESTROY => {
                for m in modules.iter_mut() { m.on_destroy(hwnd); }
                G_NID.uFlags = NIF_ICON;
                Shell_NotifyIconW(NIM_DELETE, &G_NID);
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

fn main() {
    install_panic_log();
    // Single-instance guard — exit silently if already running
    unsafe {
        use windows::Win32::System::Threading::{CreateMutexW, MUTEX_ALL_ACCESS};
        use windows::Win32::Foundation::ERROR_ALREADY_EXISTS;
        let name = w("FlagApps_SingleInstance");
        let _ = CreateMutexW(None, true, windows::core::PCWSTR(name.as_ptr()));
        if windows::Win32::Foundation::GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }
    }
    unsafe {
        let hinstance: HINSTANCE = GetModuleHandleW(None).map(|h| h.into()).unwrap_or_default();
        let class = w(MSG_CLASS);
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            lpszClassName: PCWSTR(class.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0), PCWSTR(class.as_ptr()),
            PCWSTR(w("FlagApps").as_ptr()), WS_POPUP,
            0, 0, 0, 0, None, None, hinstance, None)
            .expect("CreateWindow failed");

        // Build modules list
        let mut modules: Box<Vec<Box<dyn FlagModule>>> = Box::new(vec![
            Box::new(AnyFlag::new()),
            Box::new(KeyFlag::new()),
            Box::new(EnergyFlag::new()),
            Box::new(NewTrayFlag::new()),
            Box::new(ClipFlag::new()),
            Box::new(DeskFlag::new()),
        ]);

        // Register hwnd for keyflag winevent callback
        modules::keyflag::register_hwnd(hwnd);

        // Store modules ptr in GWLP_USERDATA.
        // NOTE: must be a pointer to the Vec itself, not Vec::as_mut_ptr() (which
        // returns a pointer to the element buffer — wndproc would then misread that
        // buffer as a Vec header and walk off into garbage memory on the first message).
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, (&mut *modules as *mut Vec<Box<dyn FlagModule>>) as isize);

        // Init modules
        for m in modules.iter_mut() { m.on_init(hwnd); }

        // Brand icon (load from resources if available, else draw)
        G_ICON_MAIN = load_app_icon(hinstance);
        G_WORK = get_work_area();

        // Set up tray icon
        G_NID = core::mem::zeroed();
        G_NID.cbSize = core::mem::size_of::<NOTIFYICONDATAW>() as u32;
        G_NID.hWnd   = hwnd;
        G_NID.uID    = 1;
        G_NID.uFlags = NIF_ICON | NIF_TIP | NIF_MESSAGE | NIF_SHOWTIP;
        G_NID.uCallbackMessage = WM_TRAY;
        G_NID.hIcon  = G_ICON_MAIN;
        G_NID.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        let tip = w("FlagApps");
        let tip_len = tip.len().min(G_NID.szTip.len());
        G_NID.szTip[..tip_len].copy_from_slice(&tip[..tip_len]);
        Shell_NotifyIconW(NIM_ADD, &G_NID);
        Shell_NotifyIconW(NIM_SETVERSION, &G_NID);

        // Create about window (hidden until invoked)
        if let Ok(about) = create_about_window(hinstance, G_ICON_MAIN.0 as isize) {
            G_ABOUT_HWND = Some(about);
        }

        // Message loop
        let mut msg = MSG::default();
        loop {
            let r = GetMessageW(&mut msg, None, 0, 0);
            if !r.as_bool() || r.0 == -1 { break; }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Keep modules alive until here so GWLP_USERDATA stays valid
        drop(modules);
    }
}

unsafe fn load_app_icon(hinstance: HINSTANCE) -> HICON {
    // 1. Embedded resource (MSVC CI builds)
    if let Ok(h) = LoadIconW(hinstance, PCWSTR(1usize as *const u16)) {
        return h;
    }
    // 2. flag.ico next to the exe (installed by the Inno Setup installer)
    if let Ok(exe) = std::env::current_exe() {
        let ico_path = exe.with_file_name("flag.ico");
        if ico_path.exists() {
            let path_w = w(&ico_path.to_string_lossy());
            if let Ok(h) = LoadImageW(None, PCWSTR(path_w.as_ptr()),
                IMAGE_ICON, 0, 0, LR_LOADFROMFILE | LR_DEFAULTSIZE) {
                return HICON(h.0 as *mut _);
            }
        }
    }
    // 3. Last resort: drawn text icon
    make_text_icon(rgb(37, 99, 235), "F")
}
