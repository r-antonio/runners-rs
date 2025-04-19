mod api;
mod config;
mod runners;
mod runners_tab;
mod backend;
mod ui;
mod cache;
mod groups_tab;

use crate::api::ApiRepository;
use crate::backend::{ApiMessage, BackendMessage, Worker};
use crate::config::read_dot_env;
use crate::groups_tab::RunnersGroupsTab;
use crate::runners::{Runner, RunnerGroup};
use crate::runners_tab::RunnersTab;
use crate::ui::Popup;
use cli_log::*;
use color_eyre::owo_colors::OwoColorize;
use color_eyre::Result;
use ratatui::widgets::Tabs;
use ratatui::{
    buffer::Buffer,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    layout::{Constraint, Layout, Rect},
    style::{
        palette::tailwind::{BLUE, GREEN, SLATE},
        Color, Modifier, Style, Stylize,
    }

    ,
    widgets::{
        ListState, Paragraph,
        StatefulWidget, Widget,
    },
    DefaultTerminal,
};
use std::fmt::{Display, Write};
use std::ops::Deref;
use std::rc::{Rc, Weak};
use std::time::Duration;
use tokio::sync::mpsc;

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

fn show_popup(popup_content: &Option<PopupInfo>, area: Rect, buf: &mut Buffer) {
    if let Some(popup) = popup_content {
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

struct AppState<'a> {
    runners_tab: RunnersTab<'a>,
    runner_groups_tab: RunnersGroupsTab<'a>,
    selected_tab: Tab,
    should_exit: bool,
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
            Tab::RunnerGroups => self.runner_groups_tab.render(main_area, buf),
        }
    }
}

impl <'a> AppState<'a> {
    fn new(runners: Vec<Runner>, runner_groups: Vec<RunnerGroup>, selected_tab: Tab, tx: &'a mpsc::UnboundedSender<BackendMessage>, api_rx: mpsc::UnboundedReceiver<ApiMessage>) -> Self {
        let mut state = AppState {
            runners_tab: RunnersTab::new(runners, tx),
            runner_groups_tab: RunnersGroupsTab::new(runner_groups, tx),
            selected_tab,
            should_exit: false,
            tx,
            api_rx
        };
        state
    }

    fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        while !self.should_exit  {
            terminal.draw(|frame| frame.render_widget(&mut self, frame.area()))?;
            if let Ok(true) = event::poll(Duration::from_millis(100)) {
                if let Event::Key(key) = event::read()? {
                    self.handle_key(key);
                };
            }
            if let Ok(message) = self.api_rx.try_recv() {
                match message {
                    ApiMessage::Ok => self.runner_groups_tab.toggle_loading(),
                    ApiMessage::RunnerList(runners) => self.set_runners(*runners),
                    ApiMessage::RunnerGroupList(groups) => self.set_runner_groups(*groups),
                    ApiMessage::GroupRepos(repos) => self.set_group_repos(*repos),
                }
            }
        }
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        if key.code == KeyCode::Tab {
            self.selected_tab = match self.selected_tab {
                Tab::Runners => Tab::RunnerGroups,
                Tab::RunnerGroups => Tab::Runners,
                a => a
            }
        }
        self.should_exit = match self.selected_tab {
            Tab::Runners => self.runners_tab.handle_input(key),
            Tab::RunnerGroups => self.runner_groups_tab.handle_input(key),
        }

    }

    fn render_header(&self, area: Rect, buf: &mut Buffer) {
        let titles = Tab::all().into_iter().map(|t|t.as_str());
        let selected_idx = Tab::all()
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

    fn set_runners(&mut self, runners: Vec<Runner>) {
        self.runners_tab.set_runners(runners);
        self.selected_tab = Tab::Runners;
    }

    fn set_runner_groups(&mut self, groups: Vec<RunnerGroup>) {
        self.runner_groups_tab.set_groups(groups);
    }

    fn set_group_repos(&mut self, repos: Vec<ApiRepository>) {
        self.runner_groups_tab.set_group_repos(repos);
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
enum Tab {
    Runners,
    RunnerGroups,
}

impl Tab {
    fn all() -> Vec<Tab> {
        vec![Tab::Runners,Tab::RunnerGroups,]
    }

    fn as_str(&self) -> &'static str {
        match self {
            Tab::Runners => " Runners ",
            Tab::RunnerGroups => " Runner Groups ",
        }
    }

    fn style(&self) -> Style {
        match self {
            Tab::Runners => TODO_HEADER_STYLE,
            Tab::RunnerGroups => TODO_HEADER_STYLE.bg(Color::Green),
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