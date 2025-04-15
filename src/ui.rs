use std::ops::Deref;
use crate::AppState;
use crate::runners::ToLine;
use crate::TEXT_FG_COLOR;
use ratatui::layout::Rect;
use ratatui::prelude::{Buffer, Line, Style, Text, Widget};
use ratatui::widgets::{Block, Borders, Clear, ListItem, ListState, Paragraph, Wrap};
use std::rc::{Rc, Weak};

pub struct UIList<T> {
    pub items: Vec<Rc<T>>,
    pub filtered_items: Vec<Weak<T>>,
    pub state: ListState,
    pub input_buffer: String,
    pub border_style: Style,
}

impl <T> UIList<T> {
    pub fn new(vec: Vec<T>, border_style: Style) -> Self {
        let items: Vec<Rc<T>> = vec.into_iter().map(Rc::new).collect();
        let filtered_items = items.iter().map(Rc::downgrade).collect();
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
    // pub fn render(&mut self, area: Rect, buf: &mut Buffer, title: &str) {
    //     let block = Block::new()
    //         .title(Line::raw(title).centered())
    //         .borders(Borders::TOP)
    //         .border_set(symbols::border::EMPTY)
    //         .border_style(self.border_style)
    //         .bg(NORMAL_ROW_BG);
    //
    //     // Iterate through all elements in the `items` and stylize them.
    //     let items: Vec<ListItem> = self
    //         .filtered_items
    //         .iter()
    //         .enumerate()
    //         .map(|(i, it)| {
    //             let color = alternate_colors(i);
    //             let item = it.upgrade().unwrap().deref();
    //             let line = item.to_line();
    //             ListItem::from(line).bg(color)
    //         })
    //         .collect();
    //
    //     // Create a List from all list items and highlight the currently selected one
    //     let list = List::new(items)
    //         .block(block)
    //         .highlight_style(SELECTED_STYLE)
    //         .highlight_symbol(">")
    //         .highlight_spacing(HighlightSpacing::Always);
    //
    //     // We need to disambiguate this trait method as both `Widget` and `StatefulWidget` share the
    //     // same method name `render`.
    //     StatefulWidget::render(list, area, buf, &mut self.state);
    // }
}

pub enum RunnerOperation {
    AddLabel,
    RemoveLabel,
    ChangeGroup,
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

impl ToLine for RunnerOperation {
    fn to_line(&self) -> Line {
        Line::styled(format!(" {}", &self.as_str()), TEXT_FG_COLOR)
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

impl ToLine for GroupOperation {
    fn to_line(&self) -> Line {
        Line::styled(format!(" {}", &self.as_str()), TEXT_FG_COLOR)
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