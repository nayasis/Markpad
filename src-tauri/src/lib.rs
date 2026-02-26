use comrak::{markdown_to_html, ComrakExtensionOptions, ComrakOptions};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri::menu::ContextMenu;
use regex::{Regex, Captures};
use std::borrow::Cow;
use serde::Serialize;

#[derive(Serialize)]
struct CleanupResult {
    deleted_count: usize,
    used_files: Vec<String>,
    checked_files: Vec<String>,
}


struct WatcherState {
    watcher: Mutex<Option<RecommendedWatcher>>,
}


mod setup;



#[tauri::command]
async fn show_window(window: tauri::Window) {
    window.show().unwrap();
}

fn process_obsidian_embeds(content: &str) -> Cow<'_, str> {
    let re = Regex::new(r"!\[\[(.*?)\]\]").unwrap();
    
    re.replace_all(content, |caps: &Captures| {
        let inner = &caps[1];
        let mut parts = inner.split('|');
        let path = parts.next().unwrap_or("");
        let size = parts.next();
        
        let path_escaped = path.replace(" ", "%20");
        
        if let Some(size_str) = size {
            if size_str.contains('x') {
                let mut dims = size_str.split('x');
                let width = dims.next().unwrap_or("");
                let height = dims.next().unwrap_or("");
                format!("<img src=\"{}\" width=\"{}\" height=\"{}\" alt=\"{}\" />", path_escaped, width, height, path)
            } else {
                format!("<img src=\"{}\" width=\"{}\" alt=\"{}\" />", path_escaped, size_str, path)
            }
        } else {
             format!("<img src=\"{}\" alt=\"{}\" />", path_escaped, path)
        }
    })
}

#[tauri::command]
fn convert_markdown(content: &str) -> String {
    let processed = process_obsidian_embeds(content);
    
    let mut options = ComrakOptions {
        extension: ComrakExtensionOptions {
            strikethrough: true,
            table: true,
            autolink: true,
            tasklist: true,
            superscript: false,
            footnotes: true,
            description_lists: true,
            ..ComrakExtensionOptions::default()
        },
        ..ComrakOptions::default()
    };
    options.render.unsafe_ = true;
    options.render.hardbreaks = true;
    options.render.sourcepos = true;

    markdown_to_html(&processed, &options)
}

#[tauri::command]
fn open_markdown(path: String) -> Result<String, String> {
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    Ok(convert_markdown(&content))
}

#[tauri::command]
fn render_markdown(content: String) -> String {
    convert_markdown(&content)
}

#[tauri::command]
fn read_file_content(path: String) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_file_content(path: String, content: String) -> Result<(), String> {
    fs::write(path, content).map_err(|e| e.to_string())
}

#[tauri::command]
fn open_file_folder(path: String) -> Result<(), String> {
    opener::reveal(path).map_err(|e| e.to_string())
}

#[tauri::command]
fn rename_file(old_path: String, new_path: String) -> Result<(), String> {
    fs::rename(old_path, new_path).map_err(|e| e.to_string())
}

#[tauri::command]
fn watch_file(handle: AppHandle, state: State<'_, WatcherState>, path: String) -> Result<(), String> {
    let mut watcher_lock = state.watcher.lock().unwrap();

    *watcher_lock = None;

    let path_to_watch = path.clone();
    let app_handle = handle.clone();

    let mut watcher = RecommendedWatcher::new(
        move |res: Result<notify::Event, notify::Error>| {
            if let Ok(_) = res {
                let _ = app_handle.emit("file-changed", ());
            }
        },
        Config::default(),
    )
    .map_err(|e| e.to_string())?;

    watcher
        .watch(Path::new(&path_to_watch), RecursiveMode::NonRecursive)
        .map_err(|e| e.to_string())?;

    *watcher_lock = Some(watcher);

    Ok(())
}

#[tauri::command]
fn unwatch_file(state: State<'_, WatcherState>) -> Result<(), String> {
    let mut watcher_lock = state.watcher.lock().unwrap();
    *watcher_lock = None;
    Ok(())
}

#[tauri::command]
fn copy_file_to_attachment(
    source_path: String,
    document_path: String,
    target_filename: String,
    is_image: bool
) -> Result<String, String> {
    use std::path::Path;

    // Validate document path
    let doc_path = Path::new(&document_path);
    if !doc_path.exists() {
        return Err("Document must be saved first".to_string());
    }

    // Determine .attachment directory path (images go to .attachment/images)
    let doc_dir = doc_path.parent()
        .ok_or("Invalid document path")?;
    let subdir = if is_image { ".attachment/images" } else { ".attachment" };
    let attach_dir = doc_dir.join(subdir);

    // Check if .attachment folder is newly created
    let attach_root = doc_dir.join(".attachment");
    let is_new_folder = !attach_root.exists();

    // Create directory
    fs::create_dir_all(&attach_dir)
        .map_err(|e| format!("Failed to create directory: {}", e))?;

    // Set hidden attribute on .attachment folder on Windows (only when newly created)
    #[cfg(target_os = "windows")]
    if is_new_folder && attach_root.exists() {
        use std::process::Command;
        let _ = Command::new("attrib")
            .args(&["+H", attach_root.to_str().unwrap_or("")])
            .output();
    }

    // Handle filename conflicts (add counter)
    let mut target_path = attach_dir.join(&target_filename);
    let mut counter = 1;

    if target_path.exists() {
        let stem = Path::new(&target_filename).file_stem()
            .and_then(|s| s.to_str()).unwrap_or("file");
        let ext = Path::new(&target_filename).extension()
            .and_then(|s| s.to_str()).unwrap_or("");

        while target_path.exists() {
            let new_name = if ext.is_empty() {
                format!("{}_{}", stem, counter)
            } else {
                format!("{}_{}.{}", stem, counter, ext)
            };
            target_path = attach_dir.join(new_name);
            counter += 1;
        }
    }

    // Copy file
    fs::copy(&source_path, &target_path)
        .map_err(|e| format!("Failed to copy file: {}", e))?;

    // Return relative path
    let filename = target_path.file_name()
        .and_then(|s| s.to_str())
        .ok_or("Invalid filename")?;
    let relative_path = format!("{}/{}", subdir, filename);
    Ok(relative_path)
}

#[tauri::command]
fn save_clipboard_image(
    document_path: String,
    image_data: Vec<u8>,
    filename: String
) -> Result<String, String> {
    use std::path::Path;

    // Validate document path
    let doc_path = Path::new(&document_path);
    if !doc_path.exists() {
        return Err("Document must be saved first".to_string());
    }

    // Create .attachment/images directory
    let doc_dir = doc_path.parent()
        .ok_or("Invalid document path")?;
    let images_dir = doc_dir.join(".attachment/images");

    // Check if .attachment folder is newly created
    let attach_root = doc_dir.join(".attachment");
    let is_new_folder = !attach_root.exists();

    fs::create_dir_all(&images_dir)
        .map_err(|e| format!("Failed to create directory: {}", e))?;

    // Set hidden attribute on .attachment folder on Windows (only when newly created)
    #[cfg(target_os = "windows")]
    if is_new_folder && attach_root.exists() {
        use std::process::Command;
        let _ = Command::new("attrib")
            .args(&["+H", attach_root.to_str().unwrap_or("")])
            .output();
    }

    // Save with date+random string filename
    let image_path = images_dir.join(&filename);

    fs::write(&image_path, image_data)
        .map_err(|e| format!("Failed to write image: {}", e))?;

    // Return relative path
    Ok(format!(".attachment/images/{}", filename))
}

#[tauri::command]
fn open_file(path: String) -> Result<(), String> {
    use std::process::Command;

    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(&["/C", "start", "", &path])
            .spawn()
            .map_err(|e| format!("Failed to open file: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open file: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open file: {}", e))?;
    }

    Ok(())
}

#[tauri::command]
fn cleanup_unused_attachments(
    document_path: String,
    content: String
) -> Result<CleanupResult, String> {
    use std::path::Path;
    use std::collections::HashSet;

    // Validate document path
    let doc_path = Path::new(&document_path);
    if !doc_path.exists() {
        return Ok(CleanupResult {
            deleted_count: 0,
            used_files: vec![],
            checked_files: vec![],
        });
    }

    let doc_dir = doc_path.parent()
        .ok_or("Invalid document path")?;
    let attach_dir = doc_dir.join(".attachment");

    // If .attachment directory doesn't exist, nothing to clean up
    if !attach_dir.exists() {
        return Ok(CleanupResult {
            deleted_count: 0,
            used_files: vec![],
            checked_files: vec![],
        });
    }

    // Helper function to generate all possible path variations (URL-encoded/decoded)
    fn add_path_variations(set: &mut HashSet<String>, path: &str) {
        // Add original path
        set.insert(path.to_string());

        // Add fully decoded path
        if let Ok(decoded) = urlencoding::decode(path) {
            let decoded_str = decoded.to_string();
            if decoded_str != path {
                set.insert(decoded_str);
            }
        }

        // Add path with only filename decoded (keep directory path as-is)
        if let Some(slash_pos) = path.rfind('/') {
            let (dir, file) = path.split_at(slash_pos + 1);
            if let Ok(decoded_file) = urlencoding::decode(file) {
                let decoded_file_str = decoded_file.to_string();
                if decoded_file_str != file {
                    set.insert(format!("{}{}", dir, decoded_file_str));
                }
            }
        }

        // Add path with only directory decoded (keep filename as-is)
        if let Some(slash_pos) = path.rfind('/') {
            let (dir, file) = path.split_at(slash_pos + 1);
            if let Ok(decoded_dir) = urlencoding::decode(dir) {
                let decoded_dir_str = decoded_dir.to_string();
                if decoded_dir_str != dir {
                    set.insert(format!("{}{}", decoded_dir_str, file));
                }
            }
        }
    }

    // Extract all attachment paths referenced in the document
    let mut used_files = HashSet::new();

    // Pattern 1: Markdown links/images with .attachment paths
    // Matches: ](.attachment/path/file.ext) - captures up to file extension, handles parentheses in filenames
    let link_re = Regex::new(r"\]\((\.attachment/.*?\.(?:jpg|jpeg|png|gif|bmp|webp|svg|pdf|doc|docx|xls|xlsx|ppt|pptx|txt|md|zip|rar|7z|tar|gz|mp3|mp4|avi|mov))\)").unwrap();
    for caps in link_re.captures_iter(&content) {
        if let Some(path) = caps.get(1) {
            let path_str = path.as_str().trim();
            add_path_variations(&mut used_files, path_str);
        }
    }

    // Pattern 2: Obsidian embeds ![[.attachment/path]] or ![[.attachment/path|size]]
    // This pattern doesn't have the parenthesis problem
    let obsidian_re = Regex::new(r"!\[\[(\.attachment/.*?\.(?:jpg|jpeg|png|gif|bmp|webp|svg|pdf|doc|docx|xls|xlsx|ppt|pptx|txt|md|zip|rar|7z|tar|gz|mp3|mp4|avi|mov))(?:\|[^\]]+)?\]\]").unwrap();
    for caps in obsidian_re.captures_iter(&content) {
        if let Some(path) = caps.get(1) {
            let path_str = path.as_str().trim();
            add_path_variations(&mut used_files, path_str);
        }
    }

    // Walk through .attachment directory and find unused files
    let mut deleted_count = 0;
    let mut checked_files = Vec::new();

    fn walk_dir(dir: &Path, base: &Path, used: &HashSet<String>, deleted: &mut usize, checked: &mut Vec<String>) -> Result<(), String> {
        if !dir.is_dir() {
            return Ok(());
        }

        let entries = fs::read_dir(dir)
            .map_err(|e| format!("Failed to read directory: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
            let path = entry.path();

            if path.is_dir() {
                walk_dir(&path, base, used, deleted, checked)?;
            } else if path.is_file() {
                // Get relative path from document directory
                let rel_path = path.strip_prefix(base)
                    .map_err(|e| format!("Failed to get relative path: {}", e))?;
                let rel_path_str = rel_path.to_string_lossy()
                    .replace("\\", "/");

                checked.push(rel_path_str.clone());

                // Check if this file is used in the document
                if !used.contains(&rel_path_str) {
                    // Delete unused file
                    if let Err(_e) = fs::remove_file(&path) {
                        // Ignore deletion errors
                    } else {
                        *deleted += 1;
                    }
                }
            }
        }

        Ok(())
    }

    walk_dir(&attach_dir, doc_dir, &used_files, &mut deleted_count, &mut checked_files)?;

    // Clean up empty directories
    fn remove_empty_dirs(dir: &Path) -> Result<(), String> {
        if !dir.is_dir() {
            return Ok(());
        }

        let entries = fs::read_dir(dir)
            .map_err(|e| format!("Failed to read directory: {}", e))?;

        let mut has_files = false;
        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
            let path = entry.path();

            if path.is_dir() {
                remove_empty_dirs(&path)?;
                // Check if dir still exists after recursive cleanup
                if path.exists() {
                    has_files = true;
                }
            } else {
                has_files = true;
            }
        }

        // Remove directory if empty
        if !has_files {
            let _ = fs::remove_dir(dir);
        }

        Ok(())
    }

    let _ = remove_empty_dirs(&attach_dir);

    Ok(CleanupResult {
        deleted_count,
        used_files: used_files.into_iter().collect(),
        checked_files,
    })
}

struct AppState {
    startup_file: Mutex<Option<String>>,
}

#[tauri::command]
fn send_markdown_path(state: State<'_, AppState>) -> Vec<String> {
    let mut files: Vec<String> = std::env::args()
        .skip(1)
        .filter(|arg| !arg.starts_with("-"))
        .collect();

    if let Some(startup_path) = state.startup_file.lock().unwrap().as_ref() {
        if !files.contains(startup_path) {
            files.insert(0, startup_path.clone());
        }
    }
    
    files
}

#[tauri::command]
async fn get_app_mode() -> String {

    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--uninstall") {
        return "uninstall".to_string();
    }

    let current_exe = std::env::current_exe().unwrap_or_default();
    let exe_name = current_exe.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
    
    let is_installer_mode = args.iter().any(|arg| arg == "--install") || exe_name.contains("installer");
    
    if setup::is_installed() {
        "app".to_string()
    } else {
        if is_installer_mode {
            "installer".to_string()
        } else {
            "app".to_string()
        }
    }
}

#[tauri::command]
fn is_win11() -> bool {
    #[cfg(target_os = "windows")]
    {
        use winreg::RegKey;
        use winreg::enums::*;

        let hklim = RegKey::predef(HKEY_LOCAL_MACHINE);
        if let Ok(current_version) = hklim.open_subkey("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion") {
            if let Ok(current_build) = current_version.get_value::<String, _>("CurrentBuild") {
                if let Ok(build_num) = current_build.parse::<u32>() {
                    return build_num >= 22000;
                }
            }
        }
    }
    false
}

#[tauri::command]
fn show_context_menu(
    app: AppHandle,
    state: State<'_, ContextMenuState>,
    window: tauri::Window,
    menu_type: String, // 'document', 'tab', 'tab_bar'
    path: Option<String>,
    tab_id: Option<String>,
    has_selection: bool,
) -> Result<(), String> {
    {
        let mut path_lock = state.active_path.lock().unwrap();
        *path_lock = path.clone();
        let mut tab_lock = state.active_tab_id.lock().unwrap();
        *tab_lock = tab_id.clone();
    }

    let menu = tauri::menu::Menu::new(&app).map_err(|e| e.to_string())?;

    match menu_type.as_str() {
        "tab" => {
            let new_tab = tauri::menu::MenuItem::with_id(&app, "ctx_tab_new", "New Tab", true, Some("Ctrl+T")).map_err(|e| e.to_string())?;
            menu.append(&new_tab).map_err(|e| e.to_string())?;

            let undo = tauri::menu::MenuItem::with_id(&app, "ctx_tab_undo", "Undo Close Tab", true, Some("Ctrl+Shift+T")).map_err(|e| e.to_string())?;
            menu.append(&undo).map_err(|e| e.to_string())?;

            let rename = tauri::menu::MenuItem::with_id(&app, "ctx_tab_rename", "Rename", true, None::<&str>).map_err(|e| e.to_string())?;
            menu.append(&rename).map_err(|e| e.to_string())?;

            let sep = tauri::menu::PredefinedMenuItem::separator(&app).map_err(|e| e.to_string())?;
            menu.append(&sep).map_err(|e| e.to_string())?;

            let close = tauri::menu::MenuItem::with_id(&app, "ctx_tab_close", "Close Tab", true, Some("Ctrl+W")).map_err(|e| e.to_string())?;
            menu.append(&close).map_err(|e| e.to_string())?;

            let close_others = tauri::menu::MenuItem::with_id(&app, "ctx_tab_close_others", "Close Other Tabs", true, None::<&str>).map_err(|e| e.to_string())?;
            menu.append(&close_others).map_err(|e| e.to_string())?;

            let close_right = tauri::menu::MenuItem::with_id(&app, "ctx_tab_close_right", "Close Tabs to Right", true, None::<&str>).map_err(|e| e.to_string())?;
            menu.append(&close_right).map_err(|e| e.to_string())?;
        },
        "tab_bar" => {
            let new_tab = tauri::menu::MenuItem::with_id(&app, "ctx_tab_new", "New Tab", true, Some("Ctrl+T")).map_err(|e| e.to_string())?;
            menu.append(&new_tab).map_err(|e| e.to_string())?;

            let undo = tauri::menu::MenuItem::with_id(&app, "ctx_tab_undo", "Undo Close Tab", true, Some("Ctrl+Shift+T")).map_err(|e| e.to_string())?;
            menu.append(&undo).map_err(|e| e.to_string())?;
        },
        _ => {
            // Document / Default
            if has_selection {
                let copy = tauri::menu::PredefinedMenuItem::copy(&app, Some("Copy")).map_err(|e| e.to_string())?;
                menu.append(&copy).map_err(|e| e.to_string())?;
            }

            let select_all = tauri::menu::PredefinedMenuItem::select_all(&app, Some("Select All")).map_err(|e| e.to_string())?;
            menu.append(&select_all).map_err(|e| e.to_string())?;

            if let Some(_) = path {
                let sep = tauri::menu::PredefinedMenuItem::separator(&app).map_err(|e| e.to_string())?;
                menu.append(&sep).map_err(|e| e.to_string())?;

                let open_folder = tauri::menu::MenuItem::with_id(&app, "ctx_open_folder", "Open File Location", true, None::<&str>).map_err(|e| e.to_string())?;
                menu.append(&open_folder).map_err(|e| e.to_string())?;

                let edit = tauri::menu::MenuItem::with_id(&app, "ctx_edit", "Edit", true, None::<&str>).map_err(|e| e.to_string())?;
                menu.append(&edit).map_err(|e| e.to_string())?;
                
                // Add separator before close
                let sep2 = tauri::menu::PredefinedMenuItem::separator(&app).map_err(|e| e.to_string())?;
                menu.append(&sep2).map_err(|e| e.to_string())?;

                let close = tauri::menu::MenuItem::with_id(&app, "ctx_close", "Close File", true, None::<&str>).map_err(|e| e.to_string())?;
                menu.append(&close).map_err(|e| e.to_string())?;
            }
        }
    }

    menu.popup(window).map_err(|e| e.to_string())?;
    Ok(())
}

struct ContextMenuState {
    active_path: Mutex<Option<String>>,
    active_tab_id: Mutex<Option<String>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "windows")]
    {
        std::env::set_var(
            "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
            "--enable-features=SmoothScrolling",
        );
    }

    tauri::Builder::default()

        .manage(AppState {
            startup_file: Mutex::new(None),
        })
        .manage(WatcherState {
            watcher: Mutex::new(None),
        })
        .manage(ContextMenuState {
            active_path: Mutex::new(None),
            active_tab_id: Mutex::new(None),
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            println!("Single Instance Args: {:?}", args);
            
            // Allow for robust finding of the file argument
            let path = args.iter().skip(1).find(|a| !a.starts_with("-")).map(|a| a.as_str()).unwrap_or("");
            
            let _ = app.get_webview_window("main").expect("no main window").emit("file-path", path);
            let _ = app.get_webview_window("main").expect("no main window").set_focus();
        }))
        .plugin(tauri_plugin_prevent_default::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .on_menu_event(|app, event| {
             let id = event.id().as_ref();
             let state = app.state::<ContextMenuState>();

             match id {
                 "ctx_open_folder" | "ctx_edit" | "ctx_close" => {
                    let path_lock = state.active_path.lock().unwrap();
                    if let Some(path) = path_lock.as_ref() {
                        match id {
                            "ctx_open_folder" => { let _ = open_file_folder(path.clone()); }
                            "ctx_edit" => {
                                if let Some(window) = app.get_webview_window("main") {
                                    let _ = window.emit("menu-edit-file", ());
                                }
                            }
                            "ctx_close" => {
                                if let Some(window) = app.get_webview_window("main") {
                                    let _ = window.emit("menu-close-file", ());
                                }
                            }
                            _ => {}
                        }
                    }
                 }
                 "ctx_tab_rename" => {
                    let tab_lock = state.active_tab_id.lock().unwrap();
                    if let Some(tab_id) = tab_lock.as_ref() {
                       if let Some(window) = app.get_webview_window("main") {
                           let _ = window.emit("menu-tab-rename", tab_id);
                       }
                    }
                 }
                 "ctx_tab_new" => {
                     if let Some(window) = app.get_webview_window("main") {
                         let _ = window.emit("menu-tab-new", ());
                     }
                 }
                 "ctx_tab_undo" => {
                     if let Some(window) = app.get_webview_window("main") {
                         let _ = window.emit("menu-tab-undo", ());
                     }
                 }
                 "ctx_tab_close" => {
                     let tab_lock = state.active_tab_id.lock().unwrap();
                     if let Some(tab_id) = tab_lock.as_ref() {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.emit("menu-tab-close", tab_id);
                        }
                     }
                 }
                 "ctx_tab_close_others" => {
                    let tab_lock = state.active_tab_id.lock().unwrap();
                    if let Some(tab_id) = tab_lock.as_ref() {
                       if let Some(window) = app.get_webview_window("main") {
                           let _ = window.emit("menu-tab-close-others", tab_id);
                       }
                    }
                 }
                 "ctx_tab_close_right" => {
                    let tab_lock = state.active_tab_id.lock().unwrap();
                    if let Some(tab_id) = tab_lock.as_ref() {
                       if let Some(window) = app.get_webview_window("main") {
                           let _ = window.emit("menu-tab-close-right", tab_id);
                       }
                    }
                 }
                 _ => {}
             }
        })
        .setup(|app| {
            let args: Vec<String> = std::env::args().collect();
            println!("Setup Args: {:?}", args);

            let current_exe = std::env::current_exe().unwrap_or_default();
            let exe_name = current_exe.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
            let is_installer_mode = args.iter().any(|arg| arg == "--install") || exe_name.contains("installer");

            let label = if is_installer_mode { "installer" } else { "main" };

            let _window = tauri::WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::App("index.html".into()))
                .title("Markpad")
                .inner_size(850.0, 650.0)
                .min_inner_size(400.0, 300.0)
                .visible(false)
                .resizable(true)
                .decorations(false)
                .shadow(false)
                .center()
                .visible(false)
                .build()?;
                
            #[cfg(target_os = "windows")]
            {
               use tauri::window::Color;
               let _ = _window.set_background_color(Some(Color(18, 18, 18, 255)));
            }

            let _ = _window.set_shadow(true);


            let window = app.get_webview_window(label).unwrap();

            let file_path = args.iter().skip(1).find(|arg| !arg.starts_with("-"));

            if let Some(path) = file_path {
                let _ = window.emit("file-path", path.as_str());
            }

            // If installer, force size (this will be saved to installer-state, not main-state)
            if is_installer_mode {
                let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize { width: 450.0, height: 550.0 }));
                let _ = window.center();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_markdown,
            render_markdown,
            send_markdown_path,
            read_file_content,
            save_file_content,

            get_app_mode,
            setup::install_app,
            setup::uninstall_app,
            setup::check_install_status,
            is_win11,
            open_file_folder,
            open_file_folder,
            rename_file,
            watch_file,
            unwatch_file,

            copy_file_to_attachment,
            save_clipboard_image,
            open_file,
            cleanup_unused_attachments,

            show_context_menu,
            show_window
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app_handle, _event| {
#[cfg(target_os = "macos")]
            if let tauri::RunEvent::Opened { urls } = _event {
                if let Some(url) = urls.first() {
                    if let Ok(path_buf) = url.to_file_path() {
                         let path_str = path_buf.to_string_lossy().to_string();
                         
                         let state = _app_handle.state::<AppState>();
                         *state.startup_file.lock().unwrap() = Some(path_str.clone());
                         
                         if let Some(window) = _app_handle.get_webview_window("main") {
                             let _ = window.emit("file-path", path_str);
                             let _ = window.set_focus();
                         }
                    }
                }
            }
        });
}
