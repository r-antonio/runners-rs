mod api;
mod config;
mod runners;
mod backend;
mod ui;

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
use std::fmt::Write;
use std::ops::Deref;
use std::rc::{Rc, Weak};
use std::time::Duration;
use color_eyre::owo_colors::OwoColorize;
use crossterm::event::KeyModifiers;
use ratatui::widgets::{BorderType, Tabs};
use tokio::sync::mpsc;
use crate::ui::{Operation, Popup};

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

struct UIList<T> {
    items: Vec<Rc<T>>,
    filtered_items: Vec<Weak<T>>,
    state: ListState,
}

impl <T> UIList<T> {
    fn new(vec: Vec<T>) -> Self {
        let items: Vec<Rc<T>> = vec.into_iter().map(Rc::new).collect();
        let filtered_items = items.iter().map(Rc::downgrade).collect();
        UIList {
            items,
            filtered_items,
            state: ListState::default(),
        }
    }
    fn with_first_selected(mut self) -> Self {
        self.select_first();
        self
    }
    fn select_first(&mut self) {
        self.state.select_first();
    }

    fn select_last(&mut self) {
        self.state.select_last();
    }
    fn select_next(&mut self) {
        self.state.select_next();
    }

    fn select_previous(&mut self) {
        self.state.select_previous();
    }

    fn select_none(&mut self) {
        self.state.select(None);
    }
}

struct AppState<'a> {
    runner_groups: Vec<RunnerGroup>,
    runners: UIList<Runner>,
    runner_ops: UIList<Operation>,
    selected_tab: Tab,
    selected_runner: Option<usize>,
    input_buffer: String,
    should_exit: bool,
    show_popup: bool,
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
            Tab::Runners => {
                self.render_list(main_area, buf);
            },
            Tab::RunnerOpSelection => {
                let idx = self.selected_runner.unwrap();
                let list_title = format!("Select operation - {}", self.runners.items[idx].name);
                let block = Block::new()
                    .title(Line::raw(list_title).centered())
                    .borders(Borders::TOP)
                    .border_set(symbols::border::EMPTY)
                    .border_style(self.selected_tab.style())
                    //.border_style(TODO_HEADER_STYLE)
                    .bg(NORMAL_ROW_BG);

                let items: Vec<ListItem> = self.runner_ops.items
                    .iter()
                    .map(|op|ListItem::from(op.deref()))
                    .collect();
                let list = List::new(items)
                    .block(block)
                    .highlight_style(SELECTED_STYLE)
                    .highlight_symbol(">")
                    .highlight_spacing(HighlightSpacing::Always);
                StatefulWidget::render(list, main_area, buf, &mut self.runner_ops.state);
                if self.show_popup {
                    let popup_area = Rect {
                        x: main_area.width / 4,
                        y: main_area.height / 3,
                        width: main_area.width / 2,
                        height: 3,
                    };
                    Popup::default()
                        .title("Input new label")
                        .content(format!("{}_", self.input_buffer.as_str()))
                        .render(popup_area, buf);
                }
            },
            Tab::RunnerGroups => {}
        }

    }
}

impl <'a> AppState<'a> {
    fn new(runners: Vec<Runner>, runner_groups: Vec<RunnerGroup>, selected_tab: Tab, tx: &'a mpsc::UnboundedSender<BackendMessage>, api_rx: mpsc::UnboundedReceiver<ApiMessage>) -> Self {
        let runner_operations = Operation::all();
        let mut state = AppState {
            runners: UIList::new(runners).with_first_selected(),
            runner_ops: UIList::new(runner_operations).with_first_selected(),
            runner_groups,
            selected_tab,
            selected_runner: None,
            input_buffer: String::new(),
            should_exit: false,
            show_popup: false,
            tx,
            api_rx
        };
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
                    ApiMessage::RunnerList(runners) => self.set_runners(runners),
                    ApiMessage::RunnerGroupList(groups) => self.set_runner_groups(groups),
                }
            }
        }
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        if key.code == KeyCode::Esc {
            if self.show_popup {
                self.show_popup = false;
            } else {
                self.should_exit = true;
            }
            return;
        }
        if key.code == KeyCode::Tab {
            self.selected_tab = match self.selected_tab {
                Tab::Runners => Tab::RunnerGroups,
                Tab::RunnerGroups => Tab::Runners,
                Tab::RunnerOpSelection => Tab::RunnerOpSelection
            }
        }
        match self.selected_tab {
            Tab::Runners => match key.code {
                KeyCode::Left => match self.selected_tab {
                    Tab::Runners => self.runners.select_none(),
                    Tab::RunnerGroups => {},
                    Tab::RunnerOpSelection => self.runner_ops.select_none(),
                },
                KeyCode::Down => self.runners.select_next(),
                KeyCode::Up => self.runners.select_previous(),
                KeyCode::Home => self.runners.select_first(),
                KeyCode::End => self.runners.select_last(),
                KeyCode::Right | KeyCode::Enter => match self.selected_tab {
                    Tab::Runners => self.toggle_status(),
                    Tab::RunnerGroups => {},
                    Tab::RunnerOpSelection => self.toggle_status(),
                },
                KeyCode::Backspace => self.remove_last_input(),
                KeyCode::Char(c) => self.update_filter(c),
                _ => {}
            },
            Tab::RunnerOpSelection => {
                match key.code {
                    KeyCode::Up => self.runner_ops.select_previous(),
                    KeyCode::Down => self.runner_ops.select_next(),
                    KeyCode::Enter => if !self.show_popup { self.show_input_popup() },
                    KeyCode::Left => self.selected_tab = Tab::Runners,
                    KeyCode::Char(c) => self.add_to_input(c),
                    KeyCode::Backspace => self.remove_last_input(),
                    _ => {}
                }
            }
            Tab::RunnerGroups => {
                match key.code {
                    _ => {}
                }
            }
        }

    }

    fn render_header(&self, area: Rect, buf: &mut Buffer) {
        let titles = Tab::menu().into_iter().map(|t|t.as_str());
        let selected_idx = Tab::menu()
            .into_iter()
            .enumerate()
            .find(|(i, tab)| self.selected_tab == *tab)
            .map(|(i, _)| i)
            .unwrap_or(0);
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

    fn render_list(&mut self, area: Rect, buf: &mut Buffer) {
        let mut list_title = String::from("Runners - ");
        list_title.push_str(self.input_buffer.as_str());
        let block = Block::new()
            .title(Line::raw(list_title).centered())
            .borders(Borders::TOP)
            .border_set(symbols::border::EMPTY)
            .border_style(self.selected_tab.style())
            .bg(NORMAL_ROW_BG);

        // Iterate through all elements in the `items` and stylize them.
        let items: Vec<ListItem> = self
            .runners
            .filtered_items
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

    fn show_input_popup(&mut self) {
        self.show_popup = true;
    }

    /// Changes the status of the selected list item
    fn toggle_status(&mut self) {
        if let Some(_) = self.runners.state.selected() {
            self.selected_runner = self.runners.state.selected();
            self.selected_tab = Tab::RunnerOpSelection;
        }
    }

    fn add_to_input(&mut self, c: char) {
        self.input_buffer.write_char(c).unwrap();
    }

    fn remove_last_input(&mut self) {
        self.input_buffer.pop();
        self.filter_items();
    }

    fn set_runners(&mut self, runners: Vec<Runner>) {
        self.runners.items = runners.into_iter().map(|r| Rc::new(r)).collect();
        self.filter_items();
    }

    fn set_runner_groups(&mut self, groups: Vec<RunnerGroup>) {
        self.runner_groups = groups;
    }

    fn update_filter(&mut self, c: char) {
        self.add_to_input(c);
        self.filter_items();
    }

    fn filter_items(&mut self) {
        self.runners.filtered_items = self.runners.items.iter()
            .filter(|r| r.name.contains(&self.input_buffer))
            .map(|r| Rc::downgrade(r))
            .collect()
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
enum Tab {
    Runners,
    RunnerGroups,
    RunnerOpSelection
}

impl Tab {
    fn menu() -> Vec<Tab> {
        vec![Tab::Runners, Tab::RunnerGroups]
    }
    fn all() -> Vec<Tab> {
        vec![Tab::Runners,Tab::RunnerGroups,Tab::RunnerOpSelection]
    }

    fn as_str(&self) -> &'static str {
        match self {
            Tab::Runners => " Runners ",
            Tab::RunnerGroups => " Runner Groups ",
            Tab::RunnerOpSelection => " RunnerOpSelection "
        }
    }

    fn style(&self) -> Style {
        match self {
            Tab::Runners => TODO_HEADER_STYLE,
            Tab::RunnerGroups => TODO_HEADER_STYLE.bg(Color::Green),
            Tab::RunnerOpSelection => TODO_HEADER_STYLE.bg(Color::Red)
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
    let mut worker = Worker::new(rx, api_tx, &config);
    color_eyre::install()?;
    let terminal = ratatui::init();

    let runners = worker.get_runners().await;
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

const fn alternate_colors(i: usize) -> Color {
    if i % 2 == 0 {
        NORMAL_ROW_BG
    } else {
        ALT_ROW_BG_COLOR
    }
}