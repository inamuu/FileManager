use filemanager::{
    entry::{FileEntry, read_directory},
    server::connect_with_macos_prompt,
    settings::Settings,
    transfer::{SharedProgress, TransferState, start_copy},
};
use gpui::{
    App, Application, Bounds, ClickEvent, Context, FocusHandle, IntoElement, KeyDownEvent,
    Modifiers, MouseButton, MouseDownEvent, Pixels, Point, PromptLevel, Render, ScrollHandle,
    SharedString, Stateful, Timer, Window, WindowBounds, WindowOptions, div, prelude::*, px,
    relative, rgb, rgba, size,
};
use std::{
    collections::BTreeSet,
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
    selected: BTreeSet<PathBuf>,
    selection_anchor: Option<usize>,
    scroll_handle: ScrollHandle,
}

impl Tab {
    fn new(path: PathBuf) -> Self {
        let entries = read_directory(&path).unwrap_or_default();
        Self {
            path,
            entries,
            selected: BTreeSet::new(),
            selection_anchor: None,
            scroll_handle: ScrollHandle::new(),
        }
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
    paths: Vec<PathBuf>,
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
            .child(if self.paths.len() > 1 {
                format!("📄  {}項目", self.paths.len())
            } else {
                format!("📄  {}", self.name)
            })
    }
}

#[derive(Clone)]
struct DraggedPane {
    index: usize,
    title: String,
}

#[derive(Clone)]
struct ContextMenu {
    position: Point<Pixels>,
    tab_index: usize,
    paths: Vec<PathBuf>,
}

impl Render for DraggedPane {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_4()
            .py_2()
            .rounded_lg()
            .bg(rgba(0xf8fbfff2))
            .border_2()
            .border_color(rgb(ACCENT))
            .shadow_lg()
            .text_sm()
            .text_color(rgb(TEXT))
            .child(format!("↔  {}", self.title))
    }
}

struct FileManager {
    tabs: Vec<Tab>,
    active_tab: usize,
    settings: Settings,
    transfers: Vec<SharedProgress>,
    status: Option<String>,
    context_menu: Option<ContextMenu>,
    focus_handle: FocusHandle,
}

impl FileManager {
    fn new(cx: &mut Context<Self>) -> Self {
        let home = home_dir();
        Self {
            tabs: vec![Tab::new(home)],
            active_tab: 0,
            settings: Settings::load(),
            transfers: Vec::new(),
            status: None,
            context_menu: None,
            focus_handle: cx.focus_handle(),
        }
    }

    fn active(&self) -> &Tab {
        &self.tabs[self.active_tab]
    }

    fn navigate_tab(&mut self, tab_index: usize, path: PathBuf, cx: &mut Context<Self>) {
        if path.is_dir() {
            self.tabs[tab_index] = Tab::new(path);
            self.active_tab = tab_index;
            self.status = None;
            cx.notify();
        }
    }

    fn navigate(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.navigate_tab(self.active_tab, path, cx);
    }

    fn open_entry(&mut self, tab_index: usize, entry: &FileEntry, cx: &mut Context<Self>) {
        if entry.is_dir {
            self.navigate_tab(tab_index, entry.path.clone(), cx);
        } else if Command::new("/usr/bin/open")
            .arg(&entry.path)
            .spawn()
            .is_err()
        {
            self.status = Some(format!("{} を開けませんでした", entry.name));
            cx.notify();
        }
    }

    fn go_back(&mut self, tab_index: usize, cx: &mut Context<Self>) {
        if let Some(parent) = self.tabs[tab_index].path.parent() {
            self.navigate_tab(tab_index, parent.to_path_buf(), cx);
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
        if index < self.active_tab {
            self.active_tab -= 1;
        } else if index == self.active_tab {
            self.active_tab = index.min(self.tabs.len() - 1);
        }
        cx.notify();
    }

    fn move_pane(&mut self, from: usize, target: usize, cx: &mut Context<Self>) {
        if from == target || from >= self.tabs.len() || target >= self.tabs.len() {
            return;
        }
        let tab = self.tabs.remove(from);
        let insert_at = target.min(self.tabs.len());
        self.tabs.insert(insert_at, tab);
        self.active_tab = insert_at;
        cx.notify();
    }

    fn select_entry(
        &mut self,
        tab_index: usize,
        row: usize,
        modifiers: Modifiers,
        cx: &mut Context<Self>,
    ) {
        let tab = &mut self.tabs[tab_index];
        if row >= tab.entries.len() {
            return;
        }
        let path = tab.entries[row].path.clone();
        if modifiers.shift {
            let anchor = tab.selection_anchor.unwrap_or(row);
            let start = anchor.min(row);
            let end = anchor.max(row);
            if !modifiers.platform {
                tab.selected.clear();
            }
            for entry in &tab.entries[start..=end] {
                tab.selected.insert(entry.path.clone());
            }
        } else if modifiers.platform {
            if !tab.selected.remove(&path) {
                tab.selected.insert(path);
            }
            tab.selection_anchor = Some(row);
        } else {
            tab.selected.clear();
            tab.selected.insert(path);
            tab.selection_anchor = Some(row);
        }
        self.active_tab = tab_index;
        self.context_menu = None;
        cx.notify();
    }

    fn click_entry(
        &mut self,
        tab_index: usize,
        row: usize,
        entry: &FileEntry,
        event: &ClickEvent,
        cx: &mut Context<Self>,
    ) {
        if !event.standard_click() {
            return;
        }
        if event.modifiers().control {
            return;
        }
        self.select_entry(tab_index, row, event.modifiers(), cx);
        if event.click_count() >= 2 {
            self.open_entry(tab_index, entry, cx);
        }
    }

    fn show_context_menu(
        &mut self,
        tab_index: usize,
        row: usize,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if !self.tabs[tab_index]
            .selected
            .contains(&self.tabs[tab_index].entries[row].path)
        {
            self.select_entry(tab_index, row, Modifiers::default(), cx);
        }
        let paths = self.tabs[tab_index].selected.iter().cloned().collect();
        self.context_menu = Some(ContextMenu {
            position,
            tab_index,
            paths,
        });
        self.active_tab = tab_index;
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let tab_index = self.active_tab;
        let tab = &self.tabs[tab_index];
        if tab.entries.is_empty() {
            return;
        }

        let current = tab.selection_anchor.or_else(|| {
            tab.entries
                .iter()
                .position(|entry| tab.selected.contains(&entry.path))
        });

        let target = match event.keystroke.key.as_str() {
            "up" => current.unwrap_or(0).saturating_sub(1),
            "down" => current
                .map(|index| (index + 1).min(tab.entries.len() - 1))
                .unwrap_or(0),
            "enter" | "return" => {
                if let Some(index) = current {
                    let entry = self.tabs[tab_index].entries[index].clone();
                    self.open_entry(tab_index, &entry, cx);
                    cx.stop_propagation();
                }
                return;
            }
            "escape" => {
                if self.context_menu.take().is_some() {
                    cx.notify();
                    cx.stop_propagation();
                }
                return;
            }
            _ => return,
        };

        let modifiers = Modifiers {
            shift: event.keystroke.modifiers.shift,
            ..Modifiers::default()
        };
        self.select_entry(tab_index, target, modifiers, cx);
        self.tabs[tab_index].scroll_handle.scroll_to_item(target);
        cx.stop_propagation();
    }

    fn open_context_item(&mut self, cx: &mut Context<Self>) {
        let Some(menu) = self.context_menu.take() else {
            return;
        };
        let entry = menu.paths.first().and_then(|path| {
            self.tabs[menu.tab_index]
                .entries
                .iter()
                .find(|entry| &entry.path == path)
                .cloned()
        });
        if let Some(entry) = entry {
            self.open_entry(menu.tab_index, &entry, cx);
        }
    }

    fn reveal_context_item(&mut self, cx: &mut Context<Self>) {
        let Some(menu) = self.context_menu.take() else {
            return;
        };
        if let Some(path) = menu.paths.first()
            && Command::new("/usr/bin/open")
                .arg("-R")
                .arg(path)
                .spawn()
                .is_err()
        {
            self.status = Some("Finderで表示できませんでした".into());
        }
        cx.notify();
    }

    fn confirm_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.context_menu.take() else {
            return;
        };
        let count = menu.paths.len();
        let prompt = window.prompt(
            PromptLevel::Warning,
            &format!("選択した{count}項目をゴミ箱に入れますか？"),
            None,
            &["ゴミ箱に入れる", "キャンセル"],
            cx,
        );
        let paths = menu.paths;
        cx.spawn(async move |this, cx| {
            if prompt.await.unwrap_or(1) != 0 {
                return;
            }
            let result = cx
                .background_spawn(async move { move_to_trash(&paths) })
                .await;
            let _ = this.update(cx, |this, cx| {
                for tab in &mut this.tabs {
                    tab.entries = read_directory(&tab.path).unwrap_or_default();
                    tab.selected.retain(|path| path.exists());
                    tab.selection_anchor = None;
                }
                this.status = result
                    .err()
                    .map(|error| format!("削除できませんでした: {error}"));
                cx.notify();
            });
        })
        .detach();
    }

    fn start_transfer(&mut self, source: PathBuf, tab_index: usize, cx: &mut Context<Self>) {
        let destination = self.tabs[tab_index].path.clone();
        if source.parent() == Some(destination.as_path()) {
            self.status = Some("同じフォルダにはコピーできません".into());
            cx.notify();
            return;
        }
        let progress = start_copy(source, destination.clone());
        self.transfers.push(progress.clone());
        self.status = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            loop {
                Timer::after(Duration::from_millis(100)).await;
                let finished = progress
                    .lock()
                    .map(|value| value.state.is_finished())
                    .unwrap_or(true);
                if this
                    .update(cx, |this, cx| {
                        if finished {
                            for tab in this.tabs.iter_mut().filter(|tab| tab.path == destination) {
                                tab.entries = read_directory(&destination).unwrap_or_default();
                            }
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
                if finished {
                    break;
                }
            }
        })
        .detach();
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

    fn render_pane_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(px(46.))
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .px_4()
            .bg(rgb(0xedeef1))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(TEXT))
                    .child("横並び表示")
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded_full()
                            .bg(rgb(0xd9dce2))
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(format!("{} ペイン", self.tabs.len())),
                    ),
            )
            .child(
                div()
                    .id("new-tab")
                    .h(px(32.))
                    .px_3()
                    .flex()
                    .items_center()
                    .rounded_md()
                    .bg(rgb(ACCENT))
                    .text_sm()
                    .text_color(rgb(0xffffff))
                    .hover(|style| style.bg(rgb(0x2468df)))
                    .cursor_pointer()
                    .child("＋  ペインを追加")
                    .on_click(cx.listener(|this, _, _, cx| this.add_tab(cx))),
            )
    }

    fn render_pane_toolbar(
        &self,
        tab_index: usize,
        tab: &Tab,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let path = tab.path.clone();
        let active = tab_index == self.active_tab;
        let dragged_pane = DraggedPane {
            index: tab_index,
            title: tab.title(),
        };
        div()
            .h(px(82.))
            .w_full()
            .flex()
            .flex_col()
            .bg(rgb(if active { 0xf8fbff } else { CARD }))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .h(px(40.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .text_sm()
                    .text_color(rgb(TEXT))
                    .child(
                        div()
                            .id(("pane-handle", tab_index))
                            .flex()
                            .items_center()
                            .gap_2()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .cursor_move()
                            .child("⠿")
                            .child("📁")
                            .child(tab.title())
                            .on_drag(dragged_pane, |pane, _, _, cx| cx.new(|_| pane.clone())),
                    )
                    .child(
                        div()
                            .id(("close-pane", tab_index))
                            .size(px(24.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_md()
                            .text_color(rgb(MUTED))
                            .hover(|style| style.bg(rgb(0xe2e5ea)))
                            .cursor_pointer()
                            .child("×")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.close_tab(tab_index, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .h(px(42.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .child(
                        toolbar_button(("back", tab_index), "‹").on_click(
                            cx.listener(move |this, _, _, cx| this.go_back(tab_index, cx)),
                        ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .h(px(30.))
                            .flex()
                            .items_center()
                            .px_3()
                            .rounded_lg()
                            .bg(rgb(BG))
                            .border_1()
                            .border_color(rgb(BORDER))
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .overflow_hidden()
                            .child(tab.path.display().to_string()),
                    )
                    .child(
                        toolbar_button(("refresh", tab_index), "↻").on_click(cx.listener(
                            move |this, _, _, cx| this.navigate_tab(tab_index, path.clone(), cx),
                        )),
                    ),
            )
    }

    fn render_file_list(
        &self,
        tab_index: usize,
        tab: &Tab,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entries = tab.entries.clone();
        div()
            .flex_1()
            .min_h(px(0.))
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
                    .child(div().w(px(90.)).child("サイズ")),
            )
            .child(
                div()
                    .id(("file-list", tab_index))
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .scrollbar_width(px(8.))
                    .track_scroll(&tab.scroll_handle)
                    .children(entries.into_iter().enumerate().map(|(index, entry)| {
                        let selected = tab.selected.contains(&entry.path);
                        let dragged = DraggedFile {
                            paths: if selected && tab.selected.len() > 1 {
                                tab.selected.iter().cloned().collect()
                            } else {
                                vec![entry.path.clone()]
                            },
                            name: entry.name.clone(),
                        };
                        let clicked = entry.clone();
                        div()
                            .id(SharedString::from(format!("entry-{tab_index}-{index}")))
                            .h(px(38.))
                            .flex()
                            .items_center()
                            .px_4()
                            .border_b_1()
                            .border_color(rgb(0xf0f1f3))
                            .text_sm()
                            .text_color(rgb(TEXT))
                            .bg(rgb(if selected { 0xd8e7ff } else { CARD }))
                            .cursor_pointer()
                            .hover(move |style| {
                                style.bg(rgb(if selected { 0xcadfff } else { 0xf1f6ff }))
                            })
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
                                    .w(px(90.))
                                    .text_color(rgb(MUTED))
                                    .child(entry.formatted_size()),
                            )
                            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                                this.focus_handle.focus(window);
                                this.click_entry(tab_index, index, &clicked, event, cx);
                            }))
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                    this.focus_handle.focus(window);
                                    this.show_context_menu(tab_index, index, event.position, cx);
                                }),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                    if event.modifiers.control {
                                        this.focus_handle.focus(window);
                                        this.show_context_menu(
                                            tab_index,
                                            index,
                                            event.position,
                                            cx,
                                        );
                                    }
                                }),
                            )
                            .on_drag(dragged, |file, _, _, cx| cx.new(|_| file.clone()))
                    })),
            )
    }

    fn render_panes(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("panes")
            .flex_1()
            .min_h(px(0.))
            .w_full()
            .flex()
            .gap_2()
            .p_2()
            .overflow_x_scroll()
            .bg(rgb(BG))
            .children(
                self.tabs
                    .iter()
                    .cloned()
                    .enumerate()
                    .map(|(tab_index, tab)| {
                        let active = tab_index == self.active_tab;
                        div()
                            .id(("pane", tab_index))
                            .min_w(px(360.))
                            .flex_1()
                            .min_h(px(0.))
                            .h_full()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .rounded_xl()
                            .bg(rgb(CARD))
                            .border_2()
                            .border_color(rgb(if active { ACCENT } else { BORDER }))
                            .shadow_sm()
                            .child(self.render_pane_toolbar(tab_index, &tab, cx))
                            .child(self.render_file_list(tab_index, &tab, cx))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.active_tab = tab_index;
                                cx.notify();
                            }))
                            .on_drop(cx.listener(move |this, file: &DraggedFile, _, cx| {
                                this.active_tab = tab_index;
                                for source in &file.paths {
                                    this.start_transfer(source.clone(), tab_index, cx);
                                }
                            }))
                            .on_drop(cx.listener(move |this, pane: &DraggedPane, _, cx| {
                                this.move_pane(pane.index, tab_index, cx);
                            }))
                    }),
            )
    }

    fn render_context_menu(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let window_size = window.bounds().size;
        div()
            .absolute()
            .top(px(0.))
            .left(px(0.))
            .size_full()
            .when_some(self.context_menu.clone(), |root, menu| {
                let menu_left = (menu.position.x - px(224.))
                    .min(window_size.width - px(452.))
                    .max(px(8.));
                let menu_top = menu
                    .position
                    .y
                    .min(window_size.height - px(150.))
                    .max(px(8.));
                root.child(
                    div()
                        .absolute()
                        .left(menu_left)
                        .top(menu_top)
                        .w(px(220.))
                        .p_1()
                        .rounded_lg()
                        .bg(rgba(0xfafafaf8))
                        .border_1()
                        .border_color(rgb(BORDER))
                        .shadow_2xl()
                        .text_sm()
                        .text_color(rgb(TEXT))
                        .child(
                            div()
                                .px_3()
                                .py_2()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child(format!("{}項目を選択中", menu.paths.len())),
                        )
                        .child(
                            context_menu_item("context-open", "開く", false)
                                .on_click(cx.listener(|this, _, _, cx| this.open_context_item(cx))),
                        )
                        .child(
                            context_menu_item("context-reveal", "Finderで表示", false).on_click(
                                cx.listener(|this, _, _, cx| this.reveal_context_item(cx)),
                            ),
                        )
                        .child(div().h(px(1.)).mx_2().my_1().bg(rgb(BORDER)))
                        .child(
                            context_menu_item("context-trash", "ゴミ箱に入れる", true).on_click(
                                cx.listener(|this, _, window, cx| this.confirm_delete(window, cx)),
                            ),
                        ),
                )
            })
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
                let transfer_id = SharedString::from(format!(
                    "transfer-{}-{}",
                    progress.source.display(),
                    progress.destination.display()
                ));
                let name = progress
                    .source
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("項目");
                let fraction = progress.fraction();
                let label = match &progress.state {
                    TransferState::Preparing => "準備中…".into(),
                    TransferState::Running => format!(
                        "{}% · {} / {}",
                        (fraction * 100.0) as u32,
                        format_bytes(progress.copied_bytes),
                        format_bytes(progress.total_bytes)
                    ),
                    TransferState::Completed => {
                        format!("完了 · {}", format_bytes(progress.total_bytes))
                    }
                    TransferState::Failed(_) => "失敗".into(),
                };
                div()
                    .id(transfer_id)
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_transfers = !self.transfers.is_empty();
        let status = self.status.clone();
        div()
            .id("filemanager-root")
            .size_full()
            .flex()
            .bg(rgb(BG))
            .font_family(".SystemUIFont")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, _| this.focus_handle.focus(window)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, window, _| this.focus_handle.focus(window)),
            )
            .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
                if event.standard_click()
                    && !event.modifiers().control
                    && this.context_menu.take().is_some()
                {
                    cx.notify();
                }
            }))
            .child(self.render_sidebar(cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(self.render_pane_bar(cx))
                    .child(self.render_panes(cx))
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
                    })
                    .child(self.render_context_menu(window, cx)),
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

fn toolbar_button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
) -> Stateful<gpui::Div> {
    let label = label.into();
    div()
        .id(id)
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

fn context_menu_item(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    danger: bool,
) -> Stateful<gpui::Div> {
    div()
        .id(id)
        .h(px(32.))
        .px_3()
        .flex()
        .items_center()
        .rounded_md()
        .text_color(rgb(if danger { 0xc73535 } else { TEXT }))
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0xe8eaf0)))
        .child(label.into())
}

fn move_to_trash(paths: &[PathBuf]) -> anyhow::Result<()> {
    let mut command = Command::new("/usr/bin/osascript");
    command
        .arg("-e")
        .arg("on run argv")
        .arg("-e")
        .arg("repeat with itemPath in argv")
        .arg("-e")
        .arg("set itemFile to POSIX file (itemPath as text)")
        .arg("-e")
        .arg("tell application \"Finder\" to delete itemFile")
        .arg("-e")
        .arg("end repeat")
        .arg("-e")
        .arg("end run");
    for path in paths {
        command.arg(path);
    }
    let output = command.output()?;
    if !output.status.success() {
        anyhow::bail!(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    Ok(())
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new("/").to_path_buf())
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.1} GB", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.1} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{bytes} B")
    }
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
