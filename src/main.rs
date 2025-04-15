mod api;
mod config;
mod runners;
mod backend;
mod ui;
mod cache;

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
use crate::api::{ApiRunnerGroupCreate, RunnerGroupVisibility};
use crate::ui::{RunnerOperation, Popup, UIList, GroupOperation};

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

struct AppState<'a> {
    runner_groups: UIList<RunnerGroup>,
    runners: UIList<Runner>,
    runner_ops: UIList<RunnerOperation>,
    group_ops: UIList<GroupOperation>,
    selected_tab: Tab,
    selected_runner: Option<usize>,
    selected_group: Option<usize>,
    selected_label: ListState,
    show_runner_labels: bool,
    input_buffer: String,
    should_exit: bool,
    show_popup: bool,
    loading: bool,
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
                self.show_popup(main_area, buf);
            },
            Tab::RemoveLabels => {
                let idx = self.selected_runner.unwrap();
                let runner = &self.runners.items[idx];
                let list_title = format!("Remove labels - {}", runner.name);
                let block = Block::new()
                    .title(Line::raw(list_title).centered())
                    .borders(Borders::TOP)
                    .border_set(symbols::border::EMPTY)
                    .border_style(self.selected_tab.style())
                    .bg(NORMAL_ROW_BG);

                let items: Vec<ListItem> = runner.labels
                    .iter()
                    .map(|label|ListItem::from(label.deref()))
                    .collect();
                let list = List::new(items)
                    .block(block)
                    .highlight_style(SELECTED_STYLE)
                    .highlight_symbol(">")
                    .highlight_spacing(HighlightSpacing::Always);
                StatefulWidget::render(list, main_area, buf, &mut self.selected_label);
                self.show_popup(main_area, buf);
            },
            Tab::RunnerGroups => {
                let list_title = format!("Runner Groups");
                let block = Block::new()
                    .title(Line::raw(list_title).centered())
                    .borders(Borders::TOP)
                    .border_set(symbols::border::EMPTY)
                    .border_style(self.selected_tab.style())
                    .bg(NORMAL_ROW_BG);

                let items: Vec<ListItem> = self.runner_groups.items
                    .iter()
                    .map(|op|ListItem::from(op.deref()))
                    .collect();
                let list = List::new(items)
                    .block(block)
                    .highlight_style(SELECTED_STYLE)
                    .highlight_symbol(">")
                    .highlight_spacing(HighlightSpacing::Always);
                StatefulWidget::render(list, main_area, buf, &mut self.runner_groups.state);
            }
            Tab::GroupOpSelection => {
                let idx = self.selected_group.unwrap();
                let list_title = format!("Select operation - {}", self.runner_groups.items[idx].name);
                let block = Block::new()
                    .title(Line::raw(list_title).centered())
                    .borders(Borders::TOP)
                    .border_set(symbols::border::EMPTY)
                    .border_style(self.selected_tab.style())
                    .bg(NORMAL_ROW_BG);

                let items: Vec<ListItem> = self.group_ops.items
                    .iter()
                    .map(|op|ListItem::from(op.deref()))
                    .collect();
                let list = List::new(items)
                    .block(block)
                    .highlight_style(SELECTED_STYLE)
                    .highlight_symbol(">")
                    .highlight_spacing(HighlightSpacing::Always);
                StatefulWidget::render(list, main_area, buf, &mut self.group_ops.state);
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
            runners: UIList::new(runners, Tab::Runners.style()).with_first_selected(),
            runner_ops: UIList::new(runner_operations, Tab::RunnerOpSelection.style()).with_first_selected(),
            runner_groups: UIList::new(runner_groups, Tab::RunnerGroups.style()).with_first_selected(),
            group_ops: UIList::new(group_operations, Tab::GroupOpSelection.style()).with_first_selected(),
            selected_tab,
            selected_runner: None,
            selected_group: None,
            selected_label: ListState::default(),
            input_buffer: String::new(),
            should_exit: false,
            show_runner_labels: false,
            show_popup: false,
            loading: false,
            tx,
            api_rx
        };
        state
    }

    fn show_popup(&self, area: Rect, buf: &mut Buffer) {
        if self.show_popup || self.loading {
            let popup_area = Rect {
                x: area.width / 4,
                y: area.height / 3,
                width: area.width / 2,
                height: 3,
            };

            if self.show_popup {
                Popup::default()
                    .title("Input new label")
                    .content(format!("{}_", self.input_buffer.as_str()))
                    .render(popup_area, buf);
            } else if self.loading {
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
                a => a
            }
        }
        match self.selected_tab {
            Tab::Runners => match key.code {
                KeyCode::Left => self.runners.select_none(),
                KeyCode::Down => self.runners.select_next(),
                KeyCode::Up => self.runners.select_previous(),
                KeyCode::Home => self.runners.select_first(),
                KeyCode::End => self.runners.select_last(),
                KeyCode::Right | KeyCode::Enter => self.advance_with_selected_runner(),
                KeyCode::Backspace => self.remove_last_input(),
                KeyCode::Char(c) => self.update_filter(c),
                _ => {}
            },
            Tab::RunnerOpSelection => {
                match key.code {
                    KeyCode::Up => self.runner_ops.select_previous(),
                    KeyCode::Down => self.runner_ops.select_next(),
                    KeyCode::Left => self.selected_tab = Tab::Runners,
                    KeyCode::Right | KeyCode::Enter => match self.runner_ops.selected() {
                        Some(RunnerOperation::AddLabel) => {
                            if !self.show_popup {
                                self.show_input_popup()
                            } else {
                                self.run_input_op()
                            }
                        },
                        Some(RunnerOperation::RemoveLabel) => {
                            self.selected_tab = Tab::RemoveLabels
                        },
                        Some(RunnerOperation::ChangeGroup) => {
                            self.selected_tab = Tab::RunnerGroups
                        }
                        _ => {}
                    },
                    KeyCode::Char(c) => self.add_to_input(c),
                    KeyCode::Backspace => self.remove_last_input(),
                    _ => {}
                }
            }
            Tab::RemoveLabels => {
                match key.code {
                    KeyCode::Up => self.selected_label.select_previous(),
                    KeyCode::Down => self.selected_label.select_next(),
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
                    _ => {}
                }
            }
            Tab::GroupOpSelection => {
                match key.code {
                    KeyCode::Up => self.group_ops.select_previous(),
                    KeyCode::Down => self.group_ops.select_next(),
                    KeyCode::Left => self.selected_tab = Tab::RunnerGroups,
                    KeyCode::Right | KeyCode::Enter => match self.group_ops.selected() {
                        Some(GroupOperation::AddRepo) => self.show_popup_or_execute(),
                        Some(GroupOperation::CreateGroup) => {
                            debug!("This should be anywhere else");
                            self.show_popup_or_execute();
                        }
                        _ => {}
                    },
                    KeyCode::Char(c) => self.add_to_input(c),
                    KeyCode::Backspace => self.remove_last_input(),
                    _ => {}
                }
            }
        }

    }

    fn show_popup_or_execute(&mut self) {
        if !self.show_popup {
            self.show_input_popup();
        } else {
            self.run_input_op();
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

    fn render_list(&mut self, area: Rect, buf: &mut Buffer) {
        let mut list_title = String::from("Runners - ");
        list_title.push_str(self.runners.input_buffer.as_str());
        //self.runners.render(area, buf, &list_title);
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
            .map(|(i, it)| {
                let color = alternate_colors(i);
                let item = it.upgrade().unwrap();
                ListItem::from(item.deref()).bg(color)
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

    fn run_input_op(&mut self) {
        self.show_popup = false;
        self.loading = true;
        let input = std::mem::replace(&mut self.input_buffer, String::new());
        match self.selected_tab {
            Tab::RunnerOpSelection => {
                let idx = self.selected_runner.unwrap();
                let runner = self.runners.filtered_items[idx].upgrade().unwrap().id;
                self.tx.send(BackendMessage::AddLabel(runner, input))
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
                        self.tx.send(BackendMessage::CreateRunnerGroup(group))
                            .expect("Could not send create runner command to backend");
                    }
                }
            }
            _ => {}
        }
    }

    fn remove_label(&mut self) {
        self.loading = true;
        let idx = self.selected_runner.unwrap();
        let selected_label = self.selected_label.selected().unwrap();
        let runner = self.runners.filtered_items[idx].upgrade().unwrap();
        let label = runner.labels[selected_label].clone();
        self.tx.send(BackendMessage::DeleteLabel(runner.id, label))
            .expect("Could not send delete label command to backend");
    }

    /// Changes the status of the selected list item
    fn advance_with_selected_runner(&mut self) {
        if let Some(_) = self.runners.state.selected() {
            self.selected_runner = self.runners.state.selected();
            self.selected_tab = Tab::RunnerOpSelection;
        }
    }

    fn advance_with_selected_group(&mut self) {
        if let Some(_) = self.runner_groups.selected() {
            self.selected_group = self.runner_groups.state.selected();
            self.selected_tab = Tab::GroupOpSelection;
        }
    }

    fn add_to_input(&mut self, c: char) {
        self.input_buffer.write_char(c).unwrap();
    }

    fn remove_last_input(&mut self) {
        self.input_buffer.pop();
        self.filter_items();
    }

    fn toggle_loading(&mut self) {
        if self.loading {
            self.loading = false;
        }
    }

    fn set_runners(&mut self, runners: Vec<Runner>) {
        self.toggle_loading();
        self.runners.items = runners.into_iter().map(|r| Rc::new(r)).collect();
        self.filter_items();
        self.selected_tab = Tab::Runners;
    }

    fn set_runner_groups(&mut self, groups: Vec<RunnerGroup>) {
        self.runner_groups.items = groups.into_iter().map(|g| Rc::new(g)).collect();
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

const fn alternate_colors(i: usize) -> Color {
    if i % 2 == 0 {
        NORMAL_ROW_BG
    } else {
        ALT_ROW_BG_COLOR
    }
}