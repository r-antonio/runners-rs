use ratatui::layout::Rect;
use ratatui::prelude::{Buffer, Line, Style, Text, Widget};
use ratatui::widgets::{Block, Borders, Clear, ListItem, Paragraph, Wrap};

pub enum Operation {
    AddLabel,
    RemoveLabel,
    DeleteLabel,
    ChangeGroup,
}

impl Operation {
    pub fn all() -> Vec<Operation> {
        vec![Operation::AddLabel, Operation::RemoveLabel, Operation::ChangeGroup]
    }
    fn as_str(&self) -> &'static str {
        match self {
            Operation::AddLabel => "Add label",
            Operation::RemoveLabel => "Remove label",
            Operation::DeleteLabel => "Delete label",
            Operation::ChangeGroup => "Change group",
        }
    }
}

impl From<&Operation> for ListItem<'_> {
    fn from(op: &Operation) -> Self {
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