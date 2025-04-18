mod api;
mod config;
mod runners;
mod runners_tab;
mod backend;
mod ui;
mod cache;

use std::cell::RefCell;
use crate::backend::{ApiMessage, BackendMessage, Worker};
use crate::config::read_dot_env;
use crate::runners::{Runner, RunnerGroup, RunnerStatus};
use cli_log::*;
use color_eyre::Result;
use ratatui::{
    buffer::Buffer,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    layout::{Constraint, Layout, Rect},
    style::{
        palette::tailwind::{BLUE, GREEN, SLATE},
        Color, Modifier, Style, Stylize,
    },
    symbols,
    text::Line,
    widgets::{
        Block, Borders, HighlightSpacing, List, ListItem, ListState, Padding, Paragraph,
        StatefulWidget, Widget, Wrap,
    },
    DefaultTerminal,
};
use std::fmt::{Display, Write};
use std::iter::Filter;
use std::ops::Deref;
use std::rc::{Rc, Weak};
use std::time::Duration;
use color_eyre::owo_colors::OwoColorize;
use crossterm::event::KeyModifiers;
use ratatui::widgets::{BorderType, Tabs};
use tokio::sync::mpsc;
use crate::api::{ApiRepository, ApiRunnerGroupCreate, RunnerGroupVisibility};
use crate::runners_tab::RunnersTab;
use crate::ui::{RunnerOperation, Popup, SelectableList, GroupOperation, FilterableList};

const TODO_HEADER_STYLE: Style = Style::new().fg(SLATE.c100).bg(BLUE.c800);
const NORMAL_ROW_BG: Color = SLATE.c950;
const ALT_ROW_BG_COLOR: Color = SLATE.c900;
const SELECTED_STYLE: Style = Style::new().bg(SLATE.c800).add_modifier(Modifier::BOLD);
const TEXT_FG_COLOR: Color = SLATE.c200;
const COMPLETED_TEXT_FG_COLOR: Color = GREEN.c500;

struct RunnerList {
    items: Vec<Rc<Runner>>,
    items_filtered: Vec<Weak<Runner>>,
    state: ListState,
}

impl RunnerList {
    fn new(items: Vec<Rc<Runner>>, state: ListState) -> Self {
        RunnerList {
            items,
            items_filtered: vec![],
            state,
        }
    }
}

struct PopupInfo {
    title: String,
    content: Box<dyn Fn() -> String>,
    is_loading: bool,
}

impl PopupInfo {
    fn loading() -> Self {
        PopupInfo {
            title: String::from("Loading"),
            content: Box::new(||String::from("Loading...")),
            is_loading: true
        }
    }

    fn new(title: String, content: String) -> Self {
        PopupInfo {
            title,
            content: Box::new(move || content.clone()),
            is_loading: false,
        }
    }

    fn new_dynamic(title: String, content_fn: Box<dyn Fn() -> String>) -> Self {
        PopupInfo {
            title,
            content: content_fn,
            is_loading: false,
        }
    }
}

struct AppState<'a> {
    runners_tab: RunnersTab,
    runner_groups: FilterableList<RunnerGroup>,
    runner_ops: SelectableList<RunnerOperation>,
    group_ops: SelectableList<GroupOperation>,
    dynamic_list: SelectableList<Box<dyn Display>>,
    selected_tab: Tab,
    selected_runner: Option<Runner>,
    selected_group: Option<RunnerGroup>,
    input_buffer: Rc<RefCell<String>>,
    should_exit: bool,
    popup_content: Option<PopupInfo>,
    tx: &'a mpsc::UnboundedSender<BackendMessage>,
    api_rx: mpsc::UnboundedReceiver<ApiMessage>,
}

impl <'a> Widget for &mut AppState<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let [header_area, main_area, footer_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
        ]).areas(area);

        self.render_header(header_area, buf);
        AppState::render_footer(footer_area, buf);
        match self.selected_tab {
            Tab::Runners => self.runners_tab.render(main_area, buf),
            Tab::RunnerOpSelection => {
                let runner = self.runners_tab.selected().unwrap();
                let list_title = format!("Select operation - {}", runner.name);
                self.runner_ops.render(main_area, buf, &list_title);
                self.show_popup(main_area, buf);
            },
            Tab::RemoveLabels => {
                let runner = self.runners_tab.selected().unwrap();
                let list_title = format!("Remove labels - {}", runner.name);
                self.dynamic_list.render(main_area, buf, &list_title);
                self.show_popup(main_area, buf);
            },
            Tab::RunnerGroups => {
                let list_title = format!("Runner Groups");
                self.runner_groups.render(main_area, buf, &list_title);
            }
            Tab::GroupOpSelection => {
                let group = self.selected_group.as_ref().unwrap();
                let list_title = format!("Select operation - {}", group.name);
                self.group_ops.render(main_area, buf, &list_title);
                self.show_popup(main_area, buf);
            }
        }

    }
}

impl <'a> AppState<'a> {
    fn new(runners: Vec<Runner>, runner_groups: Vec<RunnerGroup>, selected_tab: Tab, tx: &'a mpsc::UnboundedSender<BackendMessage>, api_rx: mpsc::UnboundedReceiver<ApiMessage>) -> Self {
        let runner_operations = RunnerOperation::all();
        let group_operations = GroupOperation::all();
        let mut state = AppState {
            runners_tab: RunnersTab::new(runners),
            runner_ops: SelectableList::new(runner_operations, Tab::RunnerOpSelection.style()).with_first_selected(),
            runner_groups: FilterableList::new(runner_groups, Tab::RunnerGroups.style()).with_first_selected(),
            group_ops: SelectableList::new(group_operations, Tab::GroupOpSelection.style()).with_first_selected(),
            dynamic_list: SelectableList::new(vec![], Tab::RemoveLabels.style()),
            selected_tab,
            selected_runner: None,
            selected_group: None,
            input_buffer: Rc::new(RefCell::new(String::new())),
            should_exit: false,
            popup_content: None,
            tx,
            api_rx
        };
        state
    }

    fn show_popup(&self, area: Rect, buf: &mut Buffer) {
        if let Some(popup) = &self.popup_content {
            let popup_area = Rect {
                x: area.width / 4,
                y: area.height / 3,
                width: area.width / 2,
                height: 3,
            };
            if !popup.is_loading {
                Popup::default()
                    .title(popup.title.as_str())
                    .content((popup.content)())
                    .render(popup_area, buf);
            } else {
                Popup::default()
                    .title("Loading")
                    .content(format!("Loading ..."))
                    .render(popup_area, buf);
            }
        }
    }

    fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        while !self.should_exit {
            terminal.draw(|frame| frame.render_widget(&mut self, frame.area()))?;
            if let Ok(true) = event::poll(Duration::from_millis(100)) {
                if let Event::Key(key) = event::read()? {
                    self.handle_key(key);
                };
            }
            if let Ok(message) = self.api_rx.try_recv() {
                match message {
                    ApiMessage::Ok => self.toggle_loading(),
                    ApiMessage::RunnerList(runners) => self.set_runners(*runners),
                    ApiMessage::RunnerGroupList(groups) => self.set_runner_groups(*groups),
                    ApiMessage::GroupRepos(repos) => self.set_group_repos(*repos),
                }
            }
        }
        Ok(())
    }

    fn add_to_input(&mut self, c: char) {
        self.input_buffer.borrow_mut().push(c);
    }

    fn remove_last_input(&mut self) {
        self.input_buffer.borrow_mut().pop();
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        if key.code == KeyCode::Esc {
            match self.popup_content {
                Some(_) => self.popup_content = None,
                None => self.should_exit = true,
            }
        }
        if key.code == KeyCode::Tab {
            self.selected_tab = match self.selected_tab {
                Tab::Runners => Tab::RunnerGroups,
                Tab::RunnerGroups => Tab::Runners,
                a => a
            }
        }
        match self.selected_tab {
            Tab::Runners => self.selected_tab = self.runners_tab.handle_input(key),
            Tab::RunnerOpSelection => {
                match key.code {
                    KeyCode::Up => self.runner_ops.select_previous(),
                    KeyCode::Down => self.runner_ops.select_next(),
                    KeyCode::Left => self.selected_tab = Tab::Runners,
                    KeyCode::Char(c) => match self.popup_content {
                        Some(_) => self.add_to_input(c),
                        _ => {}
                    }
                    KeyCode::Backspace => self.remove_last_input(),
                    KeyCode::Right | KeyCode::Enter => match self.runner_ops.selected() {
                        Some(RunnerOperation::AddLabel) => {
                            match self.popup_content {
                                Some(_) => self.run_input_op(),
                                None => {
                                    let input_clone = Rc::clone(&self.input_buffer);
                                    self.show_input_popup(
                                        PopupInfo::new_dynamic(String::from("Input new label:"),
                                                               Box::new(move || format!("{}_", input_clone.borrow()))
                                        ))
                                }
                            }
                        },
                        Some(RunnerOperation::RemoveLabel) => {
                            let runner = self.runners_tab.selected().unwrap();
                            let label_items = runner.labels
                                .iter()
                                .cloned()
                                .map(|label| Box::new(label) as Box<dyn Display>)
                                .collect();
                            self.dynamic_list.set_items(label_items);
                            self.selected_tab = Tab::RemoveLabels
                        },
                        Some(RunnerOperation::ChangeGroup) => {
                            self.selected_tab = Tab::RunnerGroups
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
            Tab::RemoveLabels => {
                match key.code {
                    KeyCode::Up => self.dynamic_list.select_previous(),
                    KeyCode::Down => self.dynamic_list.select_next(),
                    KeyCode::Left => self.selected_tab = Tab::RunnerOpSelection,
                    KeyCode::Enter => self.remove_label(),
                    _ => {}
                }
            }
            Tab::RunnerGroups => {
                match key.code {
                    KeyCode::Up => self.runner_groups.select_previous(),
                    KeyCode::Down => self.runner_groups.select_next(),
                    KeyCode::Right | KeyCode::Enter => self.advance_with_selected_group(),
                    KeyCode::Char(c) => self.runner_groups.update_filter(c),
                    KeyCode::Backspace => self.runner_groups.remove_last_input(),
                    _ => {}
                }
            }
            Tab::GroupOpSelection => {
                match key.code {
                    KeyCode::Up => self.group_ops.select_previous(),
                    KeyCode::Down => self.group_ops.select_next(),
                    KeyCode::Left => self.selected_tab = Tab::RunnerGroups,
                    KeyCode::Char(c) => match self.popup_content {
                        Some(_) => self.add_to_input(c),
                        _ => {}
                    }
                    KeyCode::Backspace => self.remove_last_input(),
                    KeyCode::Right | KeyCode::Enter => match self.group_ops.selected() {
                        Some(GroupOperation::AddRepo) => {
                            let input_clone = Rc::clone(&self.input_buffer);
                            self.show_popup_or_execute(
                                PopupInfo::new_dynamic(String::from("Input repo name:"),
                                               Box::new(move ||format!("{}_", input_clone.borrow()))
                                )
                            )
                        },
                        Some(GroupOperation::CreateGroup) => {
                            debug!("This should be anywhere else");
                            let input_clone = Rc::clone(&self.input_buffer);
                            self.show_popup_or_execute(
                                PopupInfo::new_dynamic(String::from("Input group name:"),
                                               Box::new(move || format!("{}_", input_clone.borrow()))
                                )
                            );
                        },
                        Some(GroupOperation::GetRepos) => {
                            self.run_input_op();
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
        }

    }

    fn show_popup_or_execute(&mut self, popup_info: PopupInfo) {
        match self.popup_content {
            None => self.show_input_popup(popup_info),
            _ => self.run_input_op()
        }
    }

    fn render_header(&self, area: Rect, buf: &mut Buffer) {
        let titles = Tab::menu().into_iter().map(|t|t.as_str());
        let selected_idx = Tab::menu()
            .into_iter()
            .enumerate()
            .find(|(i, tab)| self.selected_tab == *tab)
            .map(|(i, _)| i);
        Tabs::new(titles)
            .select(selected_idx)
            .padding("", "")
            .divider(" ")
            .style(Style::default()
                .bg(Color::Black)
                .fg(Color::White))
            .highlight_style(self.selected_tab.style())
            .render(area, buf);
    }

    fn render_footer(area: Rect, buf: &mut Buffer) {
        Paragraph::new("Use ↓↑ to move, ← to unselect, → to change status, g/G to go top/bottom.")
            .centered()
            .render(area, buf);
    }

    fn show_input_popup(&mut self, popup_info: PopupInfo) {
        self.popup_content = Some(popup_info);
    }

    fn run_input_op(&mut self) {
        self.popup_content = Some(PopupInfo::loading());
        let input = std::mem::replace(&mut *self.input_buffer.borrow_mut(), String::new());
        match self.selected_tab {
            Tab::RunnerOpSelection => {
                let runner = self.selected_runner.as_ref().unwrap();
                self.tx.send(BackendMessage::AddLabel(runner.id, input))
                    .expect("Could not send add label command to backend");
            }
            Tab::GroupOpSelection => {
                let op = self.group_ops.selected().unwrap();
                match op {
                    GroupOperation::AddRepo => {
                        let group = self.runner_groups.selected().unwrap();
                        self.tx.send(BackendMessage::AddRepoToGroup(input, group.id))
                            .expect("Could not send add repo command to backend");
                    }
                    GroupOperation::CreateGroup => {
                        let group = ApiRunnerGroupCreate {
                            name: input,
                            visibility: RunnerGroupVisibility::Selected,
                            runners: vec![],
                            selected_repository_ids: vec![],
                        };
                        self.tx.send(BackendMessage::CreateRunnerGroup(Box::new(group)))
                            .expect("Could not send create runner command to backend");
                    }
                    GroupOperation::GetRepos => {
                        let group = self.runner_groups.selected().unwrap();
                        self.tx.send(BackendMessage::GetGroupRepos(group.id))
                            .expect("Could not send get group repos command to backend");
                        self.selected_tab = Tab::RemoveLabels
                    }
                }
            }
            _ => {}
        }
    }

    fn remove_label(&mut self) {
        self.popup_content = Some(PopupInfo::loading());
        let runner = self.selected_runner.as_ref().unwrap();
        let selected_label = self.dynamic_list.selected().unwrap();
        let label = selected_label.to_string();
        self.tx.send(BackendMessage::DeleteLabel(runner.id, label))
            .expect("Could not send delete label command to backend");
    }

    fn advance_with_selected_group(&mut self) {
        if let Some(group) = self.runner_groups.selected() {
            self.selected_group = Some(group.clone());
            self.selected_tab = Tab::GroupOpSelection;
        }
    }

    fn toggle_loading(&mut self) {
        if let Some(popup) = &self.popup_content {
            if popup.is_loading {
                self.popup_content = None
            }
        }
    }

    fn set_runners(&mut self, runners: Vec<Runner>) {
        debug!("Setting runners...");
        self.toggle_loading();
        self.runners_tab.set_runners(runners);
        self.selected_tab = Tab::Runners;
    }

    fn set_runner_groups(&mut self, groups: Vec<RunnerGroup>) {
        self.runner_groups.items = groups.into_iter().map(|g| Rc::new(g)).collect();
        self.runner_groups.filter_items();
    }

    fn set_group_repos(&mut self, repos: Vec<ApiRepository>) {
        self.toggle_loading();
        let display_items = repos.into_iter()
            .map(|it|Box::new(it) as Box<dyn Display>)
            .collect();
        self.dynamic_list.set_items(display_items);
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
enum Tab {
    Runners,
    RunnerGroups,
    RunnerOpSelection,
    RemoveLabels,
    GroupOpSelection,
}

impl Tab {
    fn menu() -> Vec<Tab> {
        vec![Tab::Runners, Tab::RunnerGroups]
    }
    fn all() -> Vec<Tab> {
        vec![Tab::Runners,Tab::RunnerGroups,Tab::RunnerOpSelection,Tab::RemoveLabels,]
    }

    fn as_str(&self) -> &'static str {
        match self {
            Tab::Runners => " Runners ",
            Tab::RunnerGroups => " Runner Groups ",
            Tab::RunnerOpSelection => " RunnerOpSelection ",
            Tab::RemoveLabels => " Remove Labels ",
            Tab::GroupOpSelection => " GroupOpSelection ",
        }
    }

    fn style(&self) -> Style {
        match self {
            Tab::Runners => TODO_HEADER_STYLE,
            Tab::RunnerGroups => TODO_HEADER_STYLE.bg(Color::Green),
            Tab::RunnerOpSelection => TODO_HEADER_STYLE.bg(Color::Red),
            Tab::RemoveLabels => TODO_HEADER_STYLE.bg(Color::DarkGray),
            Tab::GroupOpSelection => TODO_HEADER_STYLE.bg(Color::LightRed),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    init_cli_log!();
    let config = read_dot_env()
        .expect("Could not read config file");
    let (tx, rx) = mpsc::unbounded_channel();
    let (api_tx, api_rx) = mpsc::unbounded_channel();
    let mut worker = Worker::new(rx, api_tx, config);
    color_eyre::install()?;
    let terminal = ratatui::init();

    let runners = worker.get_runners(None).await;
    let app_state = AppState::new(
        runners,
        vec!(),
        Tab::Runners,
        &tx,
        api_rx
    );

    tokio::spawn(async move {
        worker.run().await
    });

    let app_result = app_state.run(terminal);
    ratatui::restore();
    app_result
}