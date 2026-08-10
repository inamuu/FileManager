use filemanager::{
    entry::{FileEntry, read_directory},
    server::connect_with_macos_prompt,
    settings::Settings,
    transfer::{SharedProgress, TransferState, start_copy},
};
use gpui::{
    App, Application, Bounds, Context, IntoElement, Render, SharedString, Stateful, Timer, Window,
    WindowBounds, WindowOptions, div, prelude::*, px, relative, rgb, rgba, size,
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

const BG: u32 = 0xf4f5f7;
const SIDEBAR: u32 = 0xe9ebef;
const CARD: u32 = 0xffffff;
const TEXT: u32 = 0x22242a;
const MUTED: u32 = 0x727782;
const ACCENT: u32 = 0x3478f6;
const BORDER: u32 = 0xd8dbe2;

#[derive(Clone)]
struct Tab {
    path: PathBuf,
    entries: Vec<FileEntry>,
}

impl Tab {
    fn new(path: PathBuf) -> Self {
        let entries = read_directory(&path).unwrap_or_default();
        Self { path, entries }
    }

    fn title(&self) -> String {
        self.path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Macintosh HD")
            .to_owned()
    }
}

#[derive(Clone)]
struct DraggedFile {
    path: PathBuf,
    name: String,
}

impl Render for DraggedFile {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .rounded_lg()
            .bg(rgba(0xffffffee))
            .border_1()
            .border_color(rgb(BORDER))
            .shadow_lg()
            .text_color(rgb(TEXT))
            .child(format!("📄  {}", self.name))
    }
}

struct FileManager {
    tabs: Vec<Tab>,
    active_tab: usize,
    settings: Settings,
    transfers: Vec<SharedProgress>,
    status: Option<String>,
}

impl FileManager {
    fn new(cx: &mut Context<Self>) -> Self {
        cx.spawn(async move |this, cx| {
            loop {
                Timer::after(Duration::from_millis(100)).await;
                if this
                    .update(cx, |this, cx| {
                        if !this.transfers.is_empty() {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        let home = home_dir();
        Self {
            tabs: vec![Tab::new(home)],
            active_tab: 0,
            settings: Settings::load(),
            transfers: Vec::new(),
            status: None,
        }
    }

    fn active(&self) -> &Tab {
        &self.tabs[self.active_tab]
    }

    fn navigate(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if path.is_dir() {
            self.tabs[self.active_tab] = Tab::new(path);
            self.status = None;
            cx.notify();
        }
    }

    fn open_entry(&mut self, entry: &FileEntry, cx: &mut Context<Self>) {
        if entry.is_dir {
            self.navigate(entry.path.clone(), cx);
        } else if Command::new("/usr/bin/open")
            .arg(&entry.path)
            .spawn()
            .is_err()
        {
            self.status = Some(format!("{} を開けませんでした", entry.name));
            cx.notify();
        }
    }

    fn go_back(&mut self, cx: &mut Context<Self>) {
        if let Some(parent) = self.active().path.parent() {
            self.navigate(parent.to_path_buf(), cx);
        }
    }

    fn add_tab(&mut self, cx: &mut Context<Self>) {
        self.tabs.push(Tab::new(self.active().path.clone()));
        self.active_tab = self.tabs.len() - 1;
        cx.notify();
    }

    fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.tabs.len() == 1 {
            return;
        }
        self.tabs.remove(index);
        self.active_tab = self.active_tab.min(self.tabs.len() - 1);
        cx.notify();
    }

    fn start_transfer(&mut self, source: PathBuf, tab_index: usize, cx: &mut Context<Self>) {
        let destination = self.tabs[tab_index].path.clone();
        if source.parent() == Some(destination.as_path()) {
            self.status = Some("同じフォルダにはコピーできません".into());
            cx.notify();
            return;
        }
        self.transfers.push(start_copy(source, destination));
        self.status = None;
        cx.notify();
    }

    fn connect_server(&mut self, cx: &mut Context<Self>) {
        match connect_with_macos_prompt() {
            Ok(Some(server)) => {
                let path = server.mounted_path.clone();
                match self.settings.remember_server(server) {
                    Ok(()) => self.navigate(path, cx),
                    Err(error) => self.status = Some(error.to_string()),
                }
            }
            Ok(None) => {}
            Err(error) => self.status = Some(format!("接続できませんでした: {error}")),
        }
        cx.notify();
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let home = home_dir();
        let locations = vec![
            ("⌂", "ホーム", home.clone()),
            ("⇩", "ダウンロード", home.join("Downloads")),
            ("▦", "アプリケーション", PathBuf::from("/Applications")),
            ("◫", "デスクトップ", home.join("Desktop")),
            ("▤", "書類", home.join("Documents")),
        ];
        div()
            .w(px(224.))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .bg(rgb(SIDEBAR))
            .border_r_1()
            .border_color(rgb(BORDER))
            .px_3()
            .pt_5()
            .child(section_label("よく使う項目"))
            .children(locations.into_iter().map(|(icon, name, path)| {
                let selected = self.active().path == path;
                sidebar_item(icon, name, selected).on_click(cx.listener(move |this, _, _, cx| {
                    this.navigate(path.clone(), cx);
                }))
            }))
            .child(section_label("場所").mt_5())
            .children(self.settings.servers.iter().cloned().map(|server| {
                let path = server.mounted_path.clone();
                let selected = self.active().path == path;
                sidebar_item("◉", server.name, selected)
                    .on_click(cx.listener(move |this, _, _, cx| this.navigate(path.clone(), cx)))
            }))
            .child(
                sidebar_item("＋", "サーバへ接続…", false)
                    .mt_1()
                    .on_click(cx.listener(|this, _, _, cx| this.connect_server(cx))),
            )
    }

    fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(px(42.))
            .w_full()
            .flex()
            .items_end()
            .gap_1()
            .px_3()
            .bg(rgb(0xedeef1))
            .border_b_1()
            .border_color(rgb(BORDER))
            .children(self.tabs.iter().enumerate().map(|(index, tab)| {
                let active = index == self.active_tab;
                div()
                    .id(("tab", index))
                    .h(px(34.))
                    .min_w(px(140.))
                    .max_w(px(220.))
                    .px_3()
                    .flex()
                    .items_center()
                    .justify_between()
                    .rounded_t_lg()
                    .bg(rgb(if active { CARD } else { 0xe2e4e8 }))
                    .text_color(rgb(if active { TEXT } else { MUTED }))
                    .text_sm()
                    .cursor_pointer()
                    .child(format!("📁  {}", tab.title()))
                    .child(
                        div()
                            .id(("close-tab", index))
                            .px_1()
                            .rounded_sm()
                            .hover(|style| style.bg(rgb(0xd2d5db)))
                            .child("×")
                            .on_click(cx.listener(move |this, _, _, cx| this.close_tab(index, cx))),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.active_tab = index;
                        cx.notify();
                    }))
                    .on_drop(cx.listener(move |this, file: &DraggedFile, _, cx| {
                        this.start_transfer(file.path.clone(), index, cx);
                    }))
            }))
            .child(
                div()
                    .id("new-tab")
                    .h(px(30.))
                    .px_3()
                    .flex()
                    .items_center()
                    .rounded_md()
                    .text_lg()
                    .text_color(rgb(MUTED))
                    .hover(|style| style.bg(rgb(0xdfe1e5)))
                    .cursor_pointer()
                    .child("＋")
                    .on_click(cx.listener(|this, _, _, cx| this.add_tab(cx))),
            )
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(px(52.))
            .w_full()
            .flex()
            .items_center()
            .gap_2()
            .px_4()
            .bg(rgb(CARD))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(toolbar_button("‹").on_click(cx.listener(|this, _, _, cx| this.go_back(cx))))
            .child(toolbar_button("›").text_color(rgb(0xb8bbc2)))
            .child(
                div()
                    .ml_2()
                    .flex_1()
                    .h(px(32.))
                    .flex()
                    .items_center()
                    .px_3()
                    .rounded_lg()
                    .bg(rgb(BG))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .text_sm()
                    .text_color(rgb(MUTED))
                    .child(self.active().path.display().to_string()),
            )
            .child(toolbar_button("↻").on_click(cx.listener(|this, _, _, cx| {
                let path = this.active().path.clone();
                this.navigate(path, cx);
            })))
    }

    fn render_file_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let entries = self.active().entries.clone();
        div()
            .flex_1()
            .w_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(CARD))
            .child(
                div()
                    .h(px(32.))
                    .flex()
                    .items_center()
                    .px_4()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(div().flex_1().child("名前"))
                    .child(div().w(px(110.)).child("サイズ"))
                    .child(div().w(px(120.)).child("種類")),
            )
            .child(div().id("file-list").flex_1().overflow_y_scroll().children(
                entries.into_iter().enumerate().map(|(index, entry)| {
                    let dragged = DraggedFile {
                        path: entry.path.clone(),
                        name: entry.name.clone(),
                    };
                    let clicked = entry.clone();
                    div()
                        .id(("entry", index))
                        .h(px(38.))
                        .flex()
                        .items_center()
                        .px_4()
                        .border_b_1()
                        .border_color(rgb(0xf0f1f3))
                        .text_sm()
                        .text_color(rgb(TEXT))
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(0xf1f6ff)))
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .items_center()
                                .gap_3()
                                .child(div().w(px(28.)).child(entry.icon()))
                                .child(entry.name.clone()),
                        )
                        .child(
                            div()
                                .w(px(110.))
                                .text_color(rgb(MUTED))
                                .child(entry.formatted_size()),
                        )
                        .child(
                            div()
                                .w(px(120.))
                                .text_color(rgb(MUTED))
                                .child(if entry.is_dir {
                                    "フォルダ"
                                } else {
                                    "ファイル"
                                }),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| this.open_entry(&clicked, cx)))
                        .on_drag(dragged, |file, _, _, cx| cx.new(|_| file.clone()))
                }),
            ))
    }

    fn render_transfer_panel(&self) -> impl IntoElement {
        let snapshots = self
            .transfers
            .iter()
            .filter_map(|progress| progress.lock().ok().map(|value| value.clone()))
            .collect::<Vec<_>>();
        div()
            .absolute()
            .right_4()
            .bottom_4()
            .w(px(360.))
            .rounded_xl()
            .bg(rgba(0x25272df4))
            .shadow_2xl()
            .p_4()
            .text_color(rgb(0xffffff))
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("ファイル転送"),
            )
            .children(snapshots.into_iter().rev().take(4).map(|progress| {
                let name = progress
                    .source
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("項目");
                let fraction = progress.fraction();
                let label = match &progress.state {
                    TransferState::Running => format!("{}%", (fraction * 100.0) as u32),
                    TransferState::Completed => "完了".into(),
                    TransferState::Failed(_) => "失敗".into(),
                };
                div()
                    .mt_3()
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .text_xs()
                            .child(name.to_owned())
                            .child(label),
                    )
                    .child(
                        div()
                            .mt_2()
                            .h(px(5.))
                            .w_full()
                            .rounded_full()
                            .bg(rgba(0xffffff25))
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(fraction))
                                    .rounded_full()
                                    .bg(rgb(ACCENT)),
                            ),
                    )
            }))
    }
}

impl Render for FileManager {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_transfers = !self.transfers.is_empty();
        let status = self.status.clone();
        div()
            .size_full()
            .flex()
            .bg(rgb(BG))
            .font_family(".SystemUIFont")
            .child(self.render_sidebar(cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(self.render_tabs(cx))
                    .child(self.render_toolbar(cx))
                    .child(self.render_file_list(cx))
                    .when_some(status, |view, message| {
                        view.child(
                            div()
                                .absolute()
                                .left_4()
                                .bottom_4()
                                .px_4()
                                .py_2()
                                .rounded_lg()
                                .bg(rgb(0xb83b3b))
                                .text_sm()
                                .text_color(rgb(0xffffff))
                                .child(message),
                        )
                    })
                    .when(has_transfers, |view| {
                        view.child(self.render_transfer_panel())
                    }),
            )
    }
}

fn section_label(label: impl Into<SharedString>) -> gpui::Div {
    div()
        .px_2()
        .mb_1()
        .text_xs()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(rgb(MUTED))
        .child(label.into())
}

fn sidebar_item(
    icon: impl Into<SharedString>,
    name: impl Into<SharedString>,
    selected: bool,
) -> Stateful<gpui::Div> {
    let name = name.into();
    div()
        .id(name.clone())
        .h(px(34.))
        .px_2()
        .flex()
        .items_center()
        .gap_3()
        .rounded_lg()
        .text_sm()
        .text_color(rgb(TEXT))
        .cursor_pointer()
        .bg(rgb(if selected { 0xd4e2fb } else { SIDEBAR }))
        .hover(|style| style.bg(rgb(if selected { 0xcbdcf9 } else { 0xdfe1e5 })))
        .child(div().w(px(20.)).text_color(rgb(ACCENT)).child(icon.into()))
        .child(name)
}

fn toolbar_button(label: impl Into<SharedString>) -> Stateful<gpui::Div> {
    let label = label.into();
    div()
        .id(label.clone())
        .size(px(30.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_lg()
        .text_lg()
        .text_color(rgb(TEXT))
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0xe9ebef)))
        .child(label)
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new("/").to_path_buf())
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1180.), px(760.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("FileManager".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| cx.new(FileManager::new),
        )
        .expect("FileManager window could not be opened");
        cx.activate(true);
    });
}
