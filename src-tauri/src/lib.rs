use comrak::{markdown_to_html, ComrakExtensionOptions, ComrakOptions};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use regex::{Captures, Regex};
use serde::Serialize;
use std::borrow::Cow;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

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
                format!(
                    "<img src=\"{}\" width=\"{}\" height=\"{}\" alt=\"{}\" />",
                    path_escaped, width, height, path
                )
            } else {
                format!(
                    "<img src=\"{}\" width=\"{}\" alt=\"{}\" />",
                    path_escaped, size_str, path
                )
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

fn sanitize_attachment_dir_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|ch| {
            !matches!(*ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') && !ch.is_control()
        })
        .collect();

    let trimmed = cleaned
        .trim()
        .trim_end_matches(|c| c == '.' || c == ' ')
        .trim();

    if trimmed.is_empty() {
        "document".to_string()
    } else {
        trimmed.chars().take(100).collect()
    }
}

fn attachment_root_name(doc_path: &Path) -> String {
    let name = doc_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("document");
    format!(".attachment_{}", sanitize_attachment_dir_name(name))
}

fn attachment_root_dir(doc_path: &Path) -> Result<PathBuf, String> {
    let doc_dir = doc_path.parent().ok_or("Invalid document path")?;
    Ok(doc_dir.join(attachment_root_name(doc_path)))
}

#[cfg(target_os = "windows")]
fn hide_attachment_dir(path: &Path) {
    use std::process::Command;

    let _ = Command::new("attrib")
        .args(&["+H", path.to_str().unwrap_or("")])
        .output();
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.exists() {
        return Ok(());
    }

    fs::create_dir_all(destination).map_err(|e| format!("Failed to create directory: {}", e))?;

    for entry in fs::read_dir(source).map_err(|e| format!("Failed to read directory: {}", e))? {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());

        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &destination_path)?;
        } else {
            fs::copy(&source_path, &destination_path)
                .map_err(|e| format!("Failed to copy file: {}", e))?;
        }
    }

    Ok(())
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
fn prepare_save_as_content(
    source_document_path: String,
    target_document_path: String,
    content: String,
) -> Result<String, String> {
    let source_doc_path = Path::new(&source_document_path);
    let target_doc_path = Path::new(&target_document_path);

    let source_root_name = attachment_root_name(source_doc_path);
    let target_root_name = attachment_root_name(target_doc_path);

    if source_root_name == target_root_name {
        return Ok(content);
    }

    let encoded_source_root = urlencoding::encode(&source_root_name).into_owned();
    let encoded_target_root = urlencoding::encode(&target_root_name).into_owned();

    Ok(content
        .replace(
            &format!("{}/", source_root_name),
            &format!("{}/", encoded_target_root),
        )
        .replace(
            &format!("{}/", encoded_source_root),
            &format!("{}/", encoded_target_root),
        ))
}

#[tauri::command]
fn copy_attachments_for_save_as(
    source_document_path: String,
    target_document_path: String,
) -> Result<(), String> {
    let source_doc_path = Path::new(&source_document_path);
    let target_doc_path = Path::new(&target_document_path);

    let source_root_dir = attachment_root_dir(source_doc_path)?;
    let target_root_dir = attachment_root_dir(target_doc_path)?;

    if source_root_dir == target_root_dir || !source_root_dir.exists() {
        return Ok(());
    }

    copy_dir_recursive(&source_root_dir, &target_root_dir)?;

    #[cfg(target_os = "windows")]
    hide_attachment_dir(&target_root_dir);

    Ok(())
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
fn watch_file(
    handle: AppHandle,
    state: State<'_, WatcherState>,
    path: String,
) -> Result<(), String> {
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
    is_image: bool,
) -> Result<String, String> {
    // Validate document path
    let doc_path = Path::new(&document_path);
    if !doc_path.exists() {
        return Err("Document must be saved first".to_string());
    }

    let attach_root_name = attachment_root_name(doc_path);
    let attach_root = attachment_root_dir(doc_path)?;
    let attach_dir = if is_image {
        attach_root.join("images")
    } else {
        attach_root.clone()
    };
    let subdir = if is_image {
        format!("{}/images", attach_root_name)
    } else {
        attach_root_name.clone()
    };

    let is_new_folder = !attach_root.exists();

    // Create directory
    fs::create_dir_all(&attach_dir).map_err(|e| format!("Failed to create directory: {}", e))?;

    // Set hidden attribute on the attachment folder on Windows (only when newly created)
    #[cfg(target_os = "windows")]
    if is_new_folder && attach_root.exists() {
        hide_attachment_dir(&attach_root);
    }

    // Handle filename conflicts (add counter)
    let mut target_path = attach_dir.join(&target_filename);
    let mut counter = 1;

    if target_path.exists() {
        let stem = Path::new(&target_filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file");
        let ext = Path::new(&target_filename)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");

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
    fs::copy(&source_path, &target_path).map_err(|e| format!("Failed to copy file: {}", e))?;

    // Return relative path
    let filename = target_path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Invalid filename")?;
    let relative_path = format!("{}/{}", subdir, filename);
    Ok(relative_path)
}

#[tauri::command]
fn save_clipboard_image(
    document_path: String,
    image_data: Vec<u8>,
    filename: String,
) -> Result<String, String> {
    // Validate document path
    let doc_path = Path::new(&document_path);
    if !doc_path.exists() {
        return Err("Document must be saved first".to_string());
    }

    let attach_root_name = attachment_root_name(doc_path);
    let attach_root = attachment_root_dir(doc_path)?;
    let images_dir = attach_root.join("images");

    let is_new_folder = !attach_root.exists();

    fs::create_dir_all(&images_dir).map_err(|e| format!("Failed to create directory: {}", e))?;

    // Set hidden attribute on the attachment folder on Windows (only when newly created)
    #[cfg(target_os = "windows")]
    if is_new_folder && attach_root.exists() {
        hide_attachment_dir(&attach_root);
    }

    // Save with date+random string filename
    let image_path = images_dir.join(&filename);

    fs::write(&image_path, image_data).map_err(|e| format!("Failed to write image: {}", e))?;

    // Return relative path
    Ok(format!("{}/images/{}", attach_root_name, filename))
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
    content: String,
) -> Result<CleanupResult, String> {
    // Validate document path
    let doc_path = Path::new(&document_path);
    if !doc_path.exists() {
        return Ok(CleanupResult {
            deleted_count: 0,
            used_files: vec![],
            checked_files: vec![],
        });
    }

    let doc_dir = doc_path.parent().ok_or("Invalid document path")?;
    let attach_root_name = attachment_root_name(doc_path);
    let attach_dir = doc_dir.join(&attach_root_name);

    // If the current document's attachment directory doesn't exist, nothing to clean up
    if !attach_dir.exists() {
        return Ok(CleanupResult {
            deleted_count: 0,
            used_files: vec![],
            checked_files: vec![],
        });
    }

    fn path_variations(path: &str) -> Vec<String> {
        fn build_variants(
            segments: &[&str],
            index: usize,
            current: &mut Vec<String>,
            variants: &mut Vec<String>,
        ) {
            if index == segments.len() {
                variants.push(current.join("/"));
                return;
            }

            let raw = segments[index].to_string();
            current.push(raw.clone());
            build_variants(segments, index + 1, current, variants);
            current.pop();

            let encoded = urlencoding::encode(segments[index]).into_owned();
            if encoded != raw {
                current.push(encoded);
                build_variants(segments, index + 1, current, variants);
                current.pop();
            }
        }

        let segments: Vec<&str> = path.split('/').collect();
        let mut variants = Vec::new();
        build_variants(&segments, 0, &mut Vec::new(), &mut variants);

        if let Ok(decoded) = urlencoding::decode(path) {
            let decoded = decoded.into_owned();
            if !variants.contains(&decoded) {
                variants.push(decoded.clone());
            }

            let decoded_segments: Vec<&str> = decoded.split('/').collect();
            build_variants(&decoded_segments, 0, &mut Vec::new(), &mut variants);
        }

        variants.sort();
        variants.dedup();
        variants
    }

    fn is_referenced_in_content(content: &str, path: &str) -> bool {
        let markdown_link = format!("({})", path);
        let obsidian_embed = format!("[[{}]]", path);
        let obsidian_sized = format!("[[{}|", path);

        content.contains(&markdown_link)
            || content.contains(&obsidian_embed)
            || content.contains(&obsidian_sized)
    }

    let mut deleted_count = 0;
    let mut checked_files = Vec::new();
    let mut used_files = Vec::new();

    fn walk_dir(
        dir: &Path,
        base: &Path,
        content: &str,
        used: &mut Vec<String>,
        deleted: &mut usize,
        checked: &mut Vec<String>,
    ) -> Result<(), String> {
        if !dir.is_dir() {
            return Ok(());
        }

        let entries = fs::read_dir(dir).map_err(|e| format!("Failed to read directory: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
            let path = entry.path();

            if path.is_dir() {
                walk_dir(&path, base, content, used, deleted, checked)?;
            } else if path.is_file() {
                let rel_path = path
                    .strip_prefix(base)
                    .map_err(|e| format!("Failed to get relative path: {}", e))?;
                let rel_path_str = rel_path.to_string_lossy().replace("\\", "/");

                checked.push(rel_path_str.clone());

                let is_used = path_variations(&rel_path_str)
                    .iter()
                    .any(|variant| is_referenced_in_content(content, variant));

                if is_used {
                    used.push(rel_path_str);
                } else if let Err(_e) = fs::remove_file(&path) {
                    // Ignore deletion errors
                } else {
                    *deleted += 1;
                }
            }
        }

        Ok(())
    }

    walk_dir(
        &attach_dir,
        doc_dir,
        &content,
        &mut used_files,
        &mut deleted_count,
        &mut checked_files,
    )?;

    // Clean up empty directories
    fn remove_empty_dirs(dir: &Path) -> Result<(), String> {
        if !dir.is_dir() {
            return Ok(());
        }

        let entries = fs::read_dir(dir).map_err(|e| format!("Failed to read directory: {}", e))?;

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
        used_files,
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
fn save_theme(app: AppHandle, theme: String) -> Result<(), String> {
    let config_dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&config_dir).map_err(|e| e.to_string())?;
    let theme_path = config_dir.join("theme.txt");
    fs::write(theme_path, theme).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_app_mode() -> String {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--uninstall") {
        return "uninstall".to_string();
    }

    let current_exe = std::env::current_exe().unwrap_or_default();
    let exe_name = current_exe
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();

    let is_installer_mode =
        args.iter().any(|arg| arg == "--install") || exe_name.contains("installer");

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
        use winreg::enums::*;
        use winreg::RegKey;

        let hklim = RegKey::predef(HKEY_LOCAL_MACHINE);
        if let Ok(current_version) =
            hklim.open_subkey("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion")
        {
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
fn get_system_fonts() -> Vec<String> {
    use font_kit::source::SystemSource;
    let source = SystemSource::new();
    let mut families = source.all_families().unwrap_or_default();
    families.sort();
    families.dedup();
    families
}

#[tauri::command]
fn get_os_type() -> String {
    #[cfg(target_os = "macos")]
    {
        "macos".to_string()
    }
    #[cfg(target_os = "windows")]
    {
        "windows".to_string()
    }
    #[cfg(target_os = "linux")]
    {
        "linux".to_string()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        "unknown".to_string()
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "linux")]
    {
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

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
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            println!("Single Instance Args: {:?}", args);

            let path_str = args
                .iter()
                .skip(1)
                .find(|a| !a.starts_with("-"))
                .map(|a| a.as_str())
                .unwrap_or("");

            if !path_str.is_empty() {
                let path = std::path::Path::new(path_str);
                let resolved_path = if path.is_absolute() {
                    path_str.to_string()
                } else {
                    let cwd_path = std::path::Path::new(&cwd);
                    cwd_path.join(path).display().to_string()
                };

                let _ = app
                    .get_webview_window("main")
                    .expect("no main window")
                    .emit("file-path", resolved_path);
            }
            let _ = app
                .get_webview_window("main")
                .expect("no main window")
                .set_focus();
        }))
        .plugin(tauri_plugin_prevent_default::init())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::SIZE
                        | tauri_plugin_window_state::StateFlags::POSITION
                        | tauri_plugin_window_state::StateFlags::MAXIMIZED
                        | tauri_plugin_window_state::StateFlags::VISIBLE
                        | tauri_plugin_window_state::StateFlags::FULLSCREEN,
                )
                .build(),
        )
        .setup(|app| {
            let args: Vec<String> = std::env::args().collect();
            println!("Setup Args: {:?}", args);

            let current_exe = std::env::current_exe().unwrap_or_default();
            let exe_name = current_exe
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            let is_installer_mode =
                args.iter().any(|arg| arg == "--install") || exe_name.contains("installer");

            let label = if is_installer_mode {
                "installer"
            } else {
                "main"
            };

            let mut window_builder = tauri::WebviewWindowBuilder::new(
                app,
                label,
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("Markpad")
            .inner_size(900.0, 650.0)
            .min_inner_size(400.0, 300.0)
            .visible(false)
            .resizable(true)
            .shadow(false)
            .center();

            #[cfg(target_os = "macos")]
            {
                window_builder = window_builder
                    .decorations(true)
                    .title_bar_style(tauri::TitleBarStyle::Overlay)
                    .hidden_title(true);
            }

            #[cfg(not(target_os = "macos"))]
            {
                window_builder = window_builder.decorations(false);
            }

            let _window = window_builder.build()?;

            let config_dir = app.path().app_config_dir()?;
            let theme_path = config_dir.join("theme.txt");
            let theme_pref =
                fs::read_to_string(theme_path).unwrap_or_else(|_| "system".to_string());

            let window = app.get_webview_window(label).unwrap();

            let bg_color = match theme_pref.as_str() {
                "dark" => Some(tauri::window::Color(24, 24, 24, 255)),
                "light" => Some(tauri::window::Color(253, 253, 253, 255)),
                _ => {
                    if let Ok(t) = window.theme() {
                        match t {
                            tauri::Theme::Dark => Some(tauri::window::Color(24, 24, 24, 255)),
                            _ => Some(tauri::window::Color(253, 253, 253, 255)),
                        }
                    } else {
                        Some(tauri::window::Color(253, 253, 253, 255))
                    }
                }
            };

            let _ = window.set_background_color(bg_color);

            let _ = _window.set_shadow(true);

            let window = app.get_webview_window(label).unwrap();

            let file_path = args.iter().skip(1).find(|arg| !arg.starts_with("-"));

            if let Some(path) = file_path {
                let _ = window.emit("file-path", path.as_str());
            }

            // If installer, force size (this will be saved to installer-state, not main-state)
            if is_installer_mode {
                let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize {
                    width: 450.0,
                    height: 550.0,
                }));
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
            prepare_save_as_content,
            copy_attachments_for_save_as,
            get_app_mode,
            setup::install_app,
            setup::uninstall_app,
            setup::check_install_status,
            is_win11,
            open_file_folder,
            rename_file,
            watch_file,
            unwatch_file,
            show_window,
            save_theme,
            get_system_fonts,
            get_os_type,
            copy_file_to_attachment,
            save_clipboard_image,
            open_file,
            cleanup_unused_attachments,
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
