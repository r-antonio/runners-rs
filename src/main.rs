mod api;
mod config;

use tokio::sync::mpsc;
use color_eyre::Result;
use std::{thread};
use std::fmt::Write;
use std::ops::Deref;
use std::rc::{Rc, Weak};
use std::time::Duration;
use cli_log::*;
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
use reqwest::header::{HeaderMap, HeaderValue};
use crate::api::{ApiRunner, Client};
use crate::config::{read_dot_env, Config};

const TODO_HEADER_STYLE: Style = Style::new().fg(SLATE.c100).bg(BLUE.c800);
const NORMAL_ROW_BG: Color = SLATE.c950;
const ALT_ROW_BG_COLOR: Color = SLATE.c900;
const SELECTED_STYLE: Style = Style::new().bg(SLATE.c800).add_modifier(Modifier::BOLD);
const TEXT_FG_COLOR: Color = SLATE.c200;
const COMPLETED_TEXT_FG_COLOR: Color = GREEN.c500;

#[derive(Debug, Clone)]
enum RunnerStatus {
    Online,
    Offline,
    Busy,
}

#[derive(Debug, Clone)]
struct Runner {
    id: usize,
    status: RunnerStatus,
    name: String,
    labels: Vec<String>,
    group: Option<String>,
}

impl From<ApiRunner> for Runner {
    fn from(runner: ApiRunner) -> Self {
        let status = if runner.busy {
            RunnerStatus::Busy
        } else {
            RunnerStatus::Online
        };
        Runner::new(
            runner.id,
            status,
            runner.name,
            runner.labels.iter().map(|x| x.name.to_string()).collect(),
            None
        )
    }
}

impl Runner {
    fn new(id: usize, status: RunnerStatus, name: String, labels: Vec<String>, group: Option<String>) -> Self {
        Runner {
            id,
            status,
            name,
            labels,
            group,
        }
    }
}

enum RunnerGroupVisibility {
    All,
    Selected,
}

struct RunnerGroup {
    id: usize,
    name: String,
    visibility: RunnerGroupVisibility,
}

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


struct AppState<'a> {
    runners: RunnerList,
    selected_tab: Tab,
    selected_runner: Option<usize>,
    input_buffer: String,
    should_exit: bool,
    tx: &'a mpsc::UnboundedSender<BackendMessage>,
    api_rx: mpsc::UnboundedReceiver<ApiMessage>,
}

impl <'a> Widget for &mut AppState<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let [header_area, main_area, footer_area] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
            .areas(area);

        let [list_area, item_area] =
            Layout::vertical([Constraint::Fill(1), Constraint::Fill(1)]).areas(main_area);

        AppState::render_header(header_area, buf);
        AppState::render_footer(footer_area, buf);
        self.render_list(list_area, buf);
        self.render_selected_item(item_area, buf);
    }
}

impl <'a> AppState<'a> {
    fn new(runners: RunnerList, selected_tab: Tab, tx: &'a mpsc::UnboundedSender<BackendMessage>, api_rx: mpsc::UnboundedReceiver<ApiMessage>) -> Self {
        let mut state = AppState {
            runners,
            selected_tab,
            selected_runner: None,
            input_buffer: String::new(),
            should_exit: false,
            tx,
            api_rx
        };
        state.filter_items();
        state
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
                    ApiMessage::RunnerList(runners) => self.set_runners(runners)
                }
            }
        }
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        match key.code {
            KeyCode::Esc => self.should_exit = true,
            KeyCode::Left => self.select_none(),
            KeyCode::Down => self.select_next(),
            KeyCode::Up => self.select_previous(),
            KeyCode::Home => self.select_first(),
            KeyCode::End => self.select_last(),
            KeyCode::Right | KeyCode::Enter => {
                self.toggle_status();
            }
            KeyCode::Tab => self.selected_tab = match self.selected_tab {
                Tab::Runners => Tab::RunnerOpSelection,
                Tab::RunnerGroups => Tab::Runners,
                Tab::RunnerOpSelection => Tab::RunnerOpSelection
            },
            KeyCode::Backspace => self.remove_last_input(),
            KeyCode::Char(c) => self.update_filter(c),
            _ => {}
        }
    }

    fn render_header(area: Rect, buf: &mut Buffer) {
        Paragraph::new("Ratatui List Example")
            //.bold()
            .centered()
            .render(area, buf);
    }

    fn render_footer(area: Rect, buf: &mut Buffer) {
        Paragraph::new("Use ↓↑ to move, ← to unselect, → to change status, g/G to go top/bottom.")
            .centered()
            .render(area, buf);
    }

    fn render_list(&mut self, area: Rect, buf: &mut Buffer) {
        let mut list_title = String::from("TODO List - ");
        list_title.push_str(self.input_buffer.as_str());
        let block = Block::new()
            .title(Line::raw(list_title).centered())
            .borders(Borders::TOP)
            .border_set(symbols::border::EMPTY)
            .border_style(TODO_HEADER_STYLE)
            .bg(NORMAL_ROW_BG);

        // Iterate through all elements in the `items` and stylize them.
        let items: Vec<ListItem> = self
            .runners
            .items_filtered
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let color = alternate_colors(i);
                let runner = r.upgrade().unwrap();
                ListItem::from(runner.deref()).bg(color)
            })
            .collect();

        // Create a List from all list items and highlight the currently selected one
        let list = List::new(items)
            .block(block)
            .highlight_style(SELECTED_STYLE)
            .highlight_symbol(">")
            .highlight_spacing(HighlightSpacing::Always);

        // We need to disambiguate this trait method as both `Widget` and `StatefulWidget` share the
        // same method name `render`.
        StatefulWidget::render(list, area, buf, &mut self.runners.state);
    }

    fn render_selected_item(&self, area: Rect, buf: &mut Buffer) {
        // We get the info depending on the item's state.
        let info = if let Some(i) = self.runners.state.selected() {
            let runner = self.runners.items[i].as_ref();
            match runner.status {
                RunnerStatus::Online => format!("✓ DONE: {}", runner.name),
                RunnerStatus::Offline => format!("☐ TODO: {}", runner.name),
                RunnerStatus::Busy => format!("")
            }
        } else {
            "Nothing selected...".to_string()
        };

        // We show the list item's info under the list in this paragraph
        let block = Block::new()
            .title(Line::raw("TODO Info").centered())
            .borders(Borders::TOP)
            .border_set(symbols::border::EMPTY)
            .border_style(TODO_HEADER_STYLE)
            .bg(NORMAL_ROW_BG)
            .padding(Padding::horizontal(1));

        // We can now render the item info
        Paragraph::new(info)
            .block(block)
            .fg(TEXT_FG_COLOR)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }

    fn select_none(&mut self) {
        self.runners.state.select(None);
    }

    fn select_next(&mut self) {
        self.runners.state.select_next();
    }
    fn select_previous(&mut self) {
        self.runners.state.select_previous();
    }

    fn select_first(&mut self) {
        self.runners.state.select_first();
    }

    fn select_last(&mut self) {
        self.runners.state.select_last();
    }

    /// Changes the status of the selected list item
    fn toggle_status(&mut self) {
        println!("Here is supposed to select the runner and show another menu!");
        self.tx.send(BackendMessage::FetchRunners);
        // if let Some(i) = self.runners.state.selected() {
        //     let mut runner = self.runners.items[i].as_ref().borrow_mut();
        //     runner.status = match runner.status {
        //         RunnerStatus::Online => RunnerStatus::Offline,
        //         RunnerStatus::Offline => RunnerStatus::Online,
        //         RunnerStatus::Busy => RunnerStatus::Busy,
        //     }
        // }
    }

    fn remove_last_input(&mut self) {
        self.input_buffer.pop();
        self.filter_items();
    }

    fn set_runners(&mut self, runners: Vec<Runner>) {
        self.runners.items = runners.into_iter().map(|r| Rc::new(r)).collect();
        self.filter_items();
    }

    fn update_filter(&mut self, c: char) {
        self.input_buffer.write_char(c).unwrap();
        self.filter_items();
    }

    fn filter_items(&mut self) {
        self.runners.items_filtered = self.runners.items.iter()
            .filter(|r| r.name.contains(&self.input_buffer))
            .map(|r| Rc::downgrade(r))
            .collect()
    }
}

impl From<&Runner> for ListItem<'_> {
    fn from(value: &Runner) -> Self {
        let line = match value.status {
            RunnerStatus::Online => Line::styled(format!(" ☐ {}", &value.name), TEXT_FG_COLOR),
            RunnerStatus::Offline => {
                Line::styled(format!(" ✓ {}", &value.name), COMPLETED_TEXT_FG_COLOR)
            }
            RunnerStatus::Busy => Line::styled(format!(" x {}", &value.name), TEXT_FG_COLOR)
        };
        ListItem::new(line)
    }
}

#[derive(Debug, Copy, Clone)]
enum Tab {
    Runners,
    RunnerGroups,
    RunnerOpSelection
}

// messages for backend communication
enum BackendMessage {
    FetchRunners,
    AddLabel(usize, String),
    DeleteLabel(usize, String),
    ChangeGroup(usize, usize),
    AddRepoToGroup(usize, usize),
}

enum ApiMessage {
    RunnerList(Vec<Runner>),
}

#[tokio::main]
async fn main() -> Result<()> {
    init_cli_log!();
    let config = read_dot_env()
        .expect("Could not read config file");
    let (tx, rx) = mpsc::unbounded_channel();
    let (api_tx, api_rx) = mpsc::unbounded_channel();
    color_eyre::install()?;
    let terminal = ratatui::init();

    let mut app_state = AppState::new(
        RunnerList::new(vec!(), ListState::default()),
        Tab::Runners,
        &tx,
        api_rx
    );

    let worker = tokio::spawn(async move {
        backend_worker(rx, &api_tx, config).await
    });

    tx.send(BackendMessage::FetchRunners)
        .expect("Could not sent command to backend");

    let app_result = app_state.run(terminal);
    ratatui::restore();
    app_result
}

struct Worker {
    client: Client,
    rx: mpsc::UnboundedReceiver<BackendMessage>
}

impl Worker {
    fn new(client: Client, rx: mpsc::UnboundedReceiver<BackendMessage>) -> Self {
        Worker { client, rx }
    }
}

const fn alternate_colors(i: usize) -> Color {
    if i % 2 == 0 {
        NORMAL_ROW_BG
    } else {
        ALT_ROW_BG_COLOR
    }
}

async fn backend_worker(mut rx: mpsc::UnboundedReceiver<BackendMessage>, tx: &mpsc::UnboundedSender<ApiMessage>, config: Config) {
    let mut headers = HeaderMap::new();
    headers.insert("User-Agent", HeaderValue::from_str("curl").unwrap());
    headers.insert("Authorization", HeaderValue::from_str(&format!("Bearer {}", config.token)).unwrap());
    let github_client = Client::new(&format!("https://api.github.com/orgs/{}/actions/", config.organization), headers)
        .expect("Failed to create github client");

    while let Some(message) = rx.recv().await {
        match message {
            BackendMessage::FetchRunners => {
                // simulate api call
                let runners_api = github_client.runners().get_all().await.unwrap();
                debug!("Fetched runners {:?}", runners_api);
                let runners = runners_api.runners.into_iter().map(| r| Runner::from(r)).collect();
                tx.send(ApiMessage::RunnerList(runners))
                    .expect("Could not send runner list to ui");
            }
            BackendMessage::AddLabel(runner_id, label) => {
                println!("Updating label: {} for runner: {}", label, runner_id);
            }
            BackendMessage::DeleteLabel(runner_id, label) => {
                todo!()
            }
            BackendMessage::ChangeGroup(runner_id, group_id) => {
                todo!()
            }
            BackendMessage::AddRepoToGroup(repo_id, group_id) => {
                todo!()
            }
        }
    }
}