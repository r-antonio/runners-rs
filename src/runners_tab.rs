use std::rc::Rc;
use cli_log::debug;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::{Buffer, Rect};
use crate::runners::Runner;
use crate::{Tab, TODO_HEADER_STYLE};
use crate::ui::FilterableList;

pub struct RunnersTab {
    runners: FilterableList<Runner>,
}

impl RunnersTab {
    pub fn new(runners: Vec<Runner>) -> RunnersTab {
        RunnersTab {
            runners: FilterableList::new(runners, TODO_HEADER_STYLE).with_first_selected(),
        }
    }

    pub fn set_runners(&mut self, runners: Vec<Runner>) {
        self.runners.items = runners.into_iter().map(|r| Rc::new(r)).collect();
        self.runners.filter_items();
    }

    pub fn selected(&self) -> Option<&Runner> {
        self.runners.selected()
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        let mut list_title = String::from("Runners - ");
        list_title.push_str(self.runners.input_buffer.as_str());
        self.runners.render(area, buf, &list_title);
    }

    pub fn handle_input(&mut self, event: KeyEvent) -> Tab {
        match event.code {
            KeyCode::Left => self.runners.select_none(),
            KeyCode::Down => self.runners.select_next(),
            KeyCode::Up => self.runners.select_previous(),
            KeyCode::Home => self.runners.select_first(),
            KeyCode::End => self.runners.select_last(),
            KeyCode::Right | KeyCode::Enter => return Tab::RunnerOpSelection,
            KeyCode::Backspace => self.runners.remove_last_input(),
            KeyCode::Char(c) => self.runners.update_filter(c),
            _ => {}
        }
        Tab::Runners
    }
}