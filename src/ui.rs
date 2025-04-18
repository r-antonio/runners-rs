use std::fmt::{Display, Write};
use std::ops::Deref;
use crate::{ALT_ROW_BG_COLOR, NORMAL_ROW_BG, SELECTED_STYLE};
use ratatui::layout::Rect;
use ratatui::prelude::{Buffer, Color, Line, StatefulWidget, Style, Stylize, Text, Widget};
use ratatui::widgets::{Block, Borders, Clear, HighlightSpacing, List, ListItem, ListState, Paragraph, Wrap};
use std::rc::{Rc};
use color_eyre::owo_colors::OwoColorize;
use ratatui::symbols;

pub struct FilterableList<T> where T: Display {
    list: SelectableList<T>,
    pub items: Vec<Rc<T>>,
    pub input_buffer: String,
}

impl <T: Display> FilterableList<T> {
    pub fn new(items: Vec<T>, style: Style) -> Self {
        let list = SelectableList::new(items, style);
        let cloned_items = list.items.iter().map(|x| Rc::clone(x)).collect();
        FilterableList { list, items: cloned_items, input_buffer: String::new() }
    }

    pub fn with_first_selected(mut self) -> Self {
        self.select_first();
        self
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer, title: &str) {
        self.list.render(area, buf, title);
    }

    pub fn filter_items(&mut self) {
        self.list.items = self.items.iter()
            .filter(|it| it.to_string().contains(&self.input_buffer))
            .map(|it| Rc::clone(it))
            .collect();
    }

    pub fn state(&self) -> &ListState {
        &self.list.state
    }

    pub fn filtered_items(&self) -> &Vec<Rc<T>> {
        &self.list.items
    }

    pub fn select_first(&mut self) {
        self.list.select_first();
    }

    pub fn select_last(&mut self) {
        self.list.select_last();
    }
    pub fn select_next(&mut self) {
        self.list.select_next();
    }

    pub fn select_previous(&mut self) {
        self.list.select_previous();
    }

    pub fn select_none(&mut self) {
        self.list.select_none();
    }

    pub fn selected(&self) -> Option<&T> {
        self.list.selected()
    }

    pub fn update_filter(&mut self, c: char) {
        self.add_to_input(c);
        self.filter_items();
    }

    pub fn add_to_input(&mut self, c: char) {
        self.input_buffer.write_char(c).unwrap();
    }

    pub fn remove_last_input(&mut self) {
        self.input_buffer.pop();
        self.filter_items();
    }
}

pub struct SelectableList<T> where T: Display {
    pub items: Vec<Rc<T>>,
    pub state: ListState,
    pub border_style: Style,
}

impl <T: Display> SelectableList<T> {
    pub fn new(vec: Vec<T>, border_style: Style) -> Self {
        let items: Vec<Rc<T>> = vec.into_iter().map(Rc::new).collect();
        SelectableList {
            items,
            state: ListState::default(),
            border_style,
        }
    }

    pub fn set_items(&mut self, vec: Vec<T>) {
        self.items = vec.into_iter().map(Rc::new).collect();
        self.select_none();
    }

    pub fn with_first_selected(mut self) -> Self {
        self.select_first();
        self
    }
    pub fn select_first(&mut self) {
        self.state.select_first();
    }

    pub fn select_last(&mut self) {
        self.state.select_last();
    }
    pub fn select_next(&mut self) {
        self.state.select_next();
    }

    pub fn select_previous(&mut self) {
        self.state.select_previous();
    }

    pub fn select_none(&mut self) {
        self.state.select(None);
    }

    pub fn selected(&self) -> Option<&T> {
        self.state.selected().map(|idx| self.items[idx].deref())
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer, title: &str) {
        let block = Block::new()
            .title(Line::raw(title).centered())
            .borders(Borders::TOP)
            .border_set(symbols::border::EMPTY)
            .border_style(self.border_style)
            .bg(NORMAL_ROW_BG);

        // Iterate through all elements in the `items` and stylize them.
        let items: Vec<ListItem> = self
            .items
            .iter()
            .enumerate()
            .map(|(i, it)| {
                let color = alternate_colors(i);
                let item = it.deref();
                let line = Line::from(item.to_string());
                ListItem::new(line).bg(color)
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
        StatefulWidget::render(list, area, buf, &mut self.state);
    }
}

const fn alternate_colors(i: usize) -> Color {
    if i % 2 == 0 {
        NORMAL_ROW_BG
    } else {
        ALT_ROW_BG_COLOR
    }
}

pub enum RunnerOperation {
    AddLabel,
    RemoveLabel,
    ChangeGroup,
}

impl Display for RunnerOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            RunnerOperation::AddLabel => "Add label",
            RunnerOperation::RemoveLabel => "Remove label",
            RunnerOperation::ChangeGroup => "Change group",
        };
        write!(f, "{}", value)
    }
}

impl RunnerOperation {
    pub fn all() -> Vec<RunnerOperation> {
        vec![RunnerOperation::AddLabel, RunnerOperation::RemoveLabel, RunnerOperation::ChangeGroup]
    }
}

pub enum GroupOperation {
    AddRepo,
    CreateGroup,
    GetRepos,
}

impl Display for GroupOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            GroupOperation::AddRepo => "Add repo",
            GroupOperation::CreateGroup => "Create group",
            GroupOperation::GetRepos => "Get repos accesses",
        };
        write!(f, "{}", value)
    }
}

impl GroupOperation {
    pub fn all() -> Vec<GroupOperation> {
        vec![GroupOperation::CreateGroup, GroupOperation::GetRepos, GroupOperation::AddRepo]
    }
}

#[derive(Debug, Default)]
pub struct Popup<'a> {
    title: Line<'a>,
    content: Text<'a>,
    border_style: Style,
    title_style: Style,
    style: Style,
}

impl <'a> Popup<'a> {
    pub fn title<T: Into<Line<'a>>>(mut self, title: T) -> Self {
        self.title = title.into();
        self
    }
    pub fn content<T: Into<Text<'a>>>(mut self, content: T) -> Self {
        self.content = content.into();
        self
    }
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }
}

impl Widget for Popup<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // ensure that all cells under the popup are cleared to avoid leaking content
        Clear.render(area, buf);
        let block = Block::new()
            .title(self.title)
            .title_style(self.title_style)
            .borders(Borders::ALL)
            .border_style(self.border_style);
        Paragraph::new(self.content)
            .wrap(Wrap { trim: true })
            .style(self.style)
            .block(block)
            .render(area, buf);
    }
}