use std::fmt::{Display, Write};
use std::ops::Deref;
use crate::{ALT_ROW_BG_COLOR, NORMAL_ROW_BG, SELECTED_STYLE};
use ratatui::layout::Rect;
use ratatui::prelude::{Buffer, Color, Line, StatefulWidget, Style, Stylize, Text, Widget};
use ratatui::widgets::{Block, Borders, Clear, HighlightSpacing, List, ListItem, ListState, Paragraph, Wrap};
use std::rc::{Rc, Weak};
use ratatui::symbols;

pub struct UIList<T> where T: Display {
    pub items: Vec<Rc<T>>,
    pub filtered_items: Vec<Rc<T>>,
    pub state: ListState,
    pub input_buffer: String,
    pub border_style: Style,
}

impl <'a, T: 'a + Display> UIList<T> where ListItem<'a>: From<&'a T> {
    pub fn new(vec: Vec<T>, border_style: Style) -> Self {
        let items: Vec<Rc<T>> = vec.into_iter().map(Rc::new).collect();
        let filtered_items = items.iter().map(Rc::clone).collect();
        UIList {
            items,
            filtered_items,
            state: ListState::default(),
            input_buffer: String::new(),
            border_style,
        }
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

    pub fn filter_items(&mut self) {
        self.filtered_items = self.items.iter()
            .filter(|it| it.to_string().contains(&self.input_buffer))
            .map(|it| Rc::clone(it))
            .collect();
    }

    pub fn render(&'a mut self, area: Rect, buf: &mut Buffer, title: &str) {
        let block = Block::new()
            .title(Line::raw(title).centered())
            .borders(Borders::TOP)
            .border_set(symbols::border::EMPTY)
            .border_style(self.border_style)
            .bg(NORMAL_ROW_BG);

        // Iterate through all elements in the `items` and stylize them.
        let items: Vec<ListItem> = self
            .filtered_items
            .iter()
            .enumerate()
            .map(|(i, it)| {
                let color = alternate_colors(i);
                let item = it.deref();
                ListItem::from(item).bg(color)
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
        write!(f, "{}", self.as_str())
    }
}

impl RunnerOperation {
    pub fn all() -> Vec<RunnerOperation> {
        vec![RunnerOperation::AddLabel, RunnerOperation::RemoveLabel, RunnerOperation::ChangeGroup]
    }
    fn as_str(&self) -> &'static str {
        match self {
            RunnerOperation::AddLabel => "Add label",
            RunnerOperation::RemoveLabel => "Remove label",
            RunnerOperation::ChangeGroup => "Change group",
        }
    }
}

impl From<&RunnerOperation> for ListItem<'_> {
    fn from(op: &RunnerOperation) -> Self {
        ListItem::new(format!("{} ", op.as_str()))
    }
}

pub enum GroupOperation {
    AddRepo,
    CreateGroup,
}

impl Display for GroupOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl GroupOperation {
    pub fn all() -> Vec<GroupOperation> {
        vec![GroupOperation::CreateGroup, GroupOperation::AddRepo]
    }
    fn as_str(&self) -> &'static str {
        match self {
            GroupOperation::AddRepo => "Add repo",
            GroupOperation::CreateGroup => "Create group",
        }
    }
}

impl From<&GroupOperation> for ListItem<'_> {
    fn from(op: &GroupOperation) -> Self {
        ListItem::new(format!("{} ", op.as_str()))
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