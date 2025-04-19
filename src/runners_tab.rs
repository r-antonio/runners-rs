use std::cell::RefCell;
use std::fmt::Display;
use std::rc::Rc;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::{Buffer, Color, Rect};
use tokio::sync::mpsc;
use crate::runners::Runner;
use crate::{show_popup, PopupInfo, Tab, TODO_HEADER_STYLE};
use crate::backend::BackendMessage;
use crate::ui::{FilterableList, RunnerOperation, SelectableList};

enum Stage {
    SelectRunner,
    SelectOp,
    RemoveLabels,
    AddToGroup,
}

pub struct RunnersTab<'a> {
    runners: FilterableList<Runner>,
    operations: SelectableList<RunnerOperation>,
    dynamic_list: SelectableList<Box<dyn Display>>,
    stage: Stage,
    input_buffer: Rc<RefCell<String>>,
    popup_content: Option<PopupInfo>,
    tx: &'a mpsc::UnboundedSender<BackendMessage>,
}

impl <'a> RunnersTab<'a> {
    pub fn new(runners: Vec<Runner>, tx: &mpsc::UnboundedSender<BackendMessage>) -> RunnersTab {
        RunnersTab {
            runners: FilterableList::new(runners, TODO_HEADER_STYLE).with_first_selected(),
            operations: SelectableList::new(RunnerOperation::all(), TODO_HEADER_STYLE.bg(Color::Red)).with_first_selected(),
            stage: Stage::SelectRunner,
            dynamic_list: SelectableList::new(vec![], TODO_HEADER_STYLE),
            input_buffer: Rc::new(RefCell::new(String::new())),
            popup_content: None,
            tx
        }
    }

    fn toggle_loading(&mut self) {
        if let Some(popup) = &self.popup_content {
            if popup.is_loading {
                self.popup_content = None
            }
        }
    }

    pub fn set_runners(&mut self, runners: Vec<Runner>) {
        self.runners.items = runners.into_iter().map(|r| Rc::new(r)).collect();
        self.runners.filter_items();
        self.toggle_loading();
        self.stage = Stage::SelectRunner;
    }

    pub fn selected(&self) -> Option<&Runner> {
        self.runners.selected()
    }

    fn add_to_input(&mut self, c: char) {
        self.input_buffer.borrow_mut().push(c);
    }

    fn remove_last_input(&mut self) {
        self.input_buffer.borrow_mut().pop();
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        match self.stage {
            Stage::SelectRunner => {
                let mut list_title = String::from("Runners - ");
                list_title.push_str(self.runners.input_buffer.as_str());
                self.runners.render(area, buf, &list_title);
            }
            Stage::SelectOp => {
                let runner = self.selected().unwrap();
                let list_title = format!("Select operation - {}", runner.name);
                self.operations.render(area, buf, &list_title);
            },
            Stage::RemoveLabels => {
                let runner = self.selected().unwrap();
                let list_title = format!("Remove labels - {}", runner.name);
                self.dynamic_list.render(area, buf, &list_title);
            },
            Stage::AddToGroup => {
                //let groups = self.runners.items.iter().map(|r|r.group).collect();
            }
        }
        show_popup(&self.popup_content, area, buf);
    }

    fn add_label(&mut self) {
        self.popup_content = Some(PopupInfo::loading());
        let input = std::mem::replace(&mut *self.input_buffer.borrow_mut(), String::new());
        let runner = self.selected().unwrap();
        self.tx.send(BackendMessage::AddLabel(runner.id, input))
            .expect("Could not send add label command to backend");
    }

    fn remove_label(&mut self) {
        self.popup_content = Some(PopupInfo::loading());
        let runner = self.selected().unwrap();
        let selected_label = self.dynamic_list.selected().unwrap();
        let label = selected_label.to_string();
        self.tx.send(BackendMessage::DeleteLabel(runner.id, label))
            .expect("Could not send delete label command to backend");
    }

    fn add_to_group(&mut self) {
        self.popup_content = Some(PopupInfo::loading());
        let runner = self.selected().unwrap();
        let selected_group = self.dynamic_list.selected().unwrap();
        let group = selected_group.to_string();
        // self.tx.send(BackendMessage::AddRunnerToGroup(runner.id, group))
        //     .expect("Could not send delete label command to backend");
    }

    pub fn handle_input(&mut self, event: KeyEvent) -> bool {
        if event.code == KeyCode::Esc && self.popup_content.is_none() {
            return true;
        }
        match self.stage {
            Stage::SelectRunner => {
                match event.code {
                    KeyCode::Left => self.runners.select_none(),
                    KeyCode::Down => self.runners.select_next(),
                    KeyCode::Up => self.runners.select_previous(),
                    KeyCode::Home => self.runners.select_first(),
                    KeyCode::End => self.runners.select_last(),
                    KeyCode::Right | KeyCode::Enter => {
                        self.stage = Stage::SelectOp;
                    },
                    KeyCode::Backspace => self.runners.remove_last_input(),
                    KeyCode::Char(c) => self.runners.update_filter(c),
                    _ => {}
                }
            }
            Stage::SelectOp => {
                match event.code {
                    KeyCode::Up => self.operations.select_previous(),
                    KeyCode::Down => self.operations.select_next(),
                    KeyCode::Left => self.stage = Stage::SelectRunner,
                    KeyCode::Char(c) => match self.popup_content {
                        Some(_) => self.add_to_input(c),
                        _ => {}
                    }
                    KeyCode::Backspace => self.remove_last_input(),
                    KeyCode::Right | KeyCode::Enter => match self.operations.selected() {
                        Some(RunnerOperation::AddLabel) => {
                            match self.popup_content {
                                Some(_) => self.add_label(),
                                None => {
                                    let input_clone = Rc::clone(&self.input_buffer);
                                    self.popup_content = Some(
                                        PopupInfo::new_dynamic(String::from("Input new label:"),
                                                               Box::new(move || format!("{}_", input_clone.borrow()))
                                        ))
                                }
                            }
                        },
                        Some(RunnerOperation::RemoveLabel) => {
                            let runner = self.selected().unwrap();
                            let label_items = runner.labels
                                .iter()
                                .cloned()
                                .map(|label| Box::new(label) as Box<dyn Display>)
                                .collect();
                            self.dynamic_list.set_items(label_items);
                            self.stage = Stage::RemoveLabels
                        },
                        Some(RunnerOperation::ChangeGroup) => {
                            self.stage = Stage::AddToGroup
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
            Stage::RemoveLabels => {
                match event.code {
                    KeyCode::Up => self.dynamic_list.select_previous(),
                    KeyCode::Down => self.dynamic_list.select_next(),
                    KeyCode::Left => self.stage = Stage::SelectOp,
                    KeyCode::Enter => self.remove_label(),
                    _ => {}
                }
            }
            Stage::AddToGroup => {
                match event.code {
                    KeyCode::Up => self.dynamic_list.select_previous(),
                    KeyCode::Down => self.dynamic_list.select_next(),
                    KeyCode::Left => self.stage = Stage::SelectOp,
                    KeyCode::Enter => self.add_to_group(),
                    _ => {}
                }
            }
        }
        false
    }
}