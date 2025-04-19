use std::cell::RefCell;
use std::fmt::Display;
use std::rc::Rc;
use cli_log::debug;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::Color;
use tokio::sync::mpsc;
use crate::model::runners::{GroupOperation, RunnerGroup};
use crate::{show_popup, PopupInfo, Tab, TODO_HEADER_STYLE};
use crate::client::api::{ApiRepository, ApiRunnerGroupCreate, RunnerGroupVisibility};
use crate::backend::BackendMessage;
use crate::ui::{FilterableList, SelectableList};

enum Stage {
    SelectGroup,
    SelectOperation,
    CreateGroup,
    AddRepo,
    ListRepos,
}

pub struct RunnersGroupsTab<'a> {
    groups: FilterableList<RunnerGroup>,
    operations: SelectableList<GroupOperation>,
    dynamic_list: SelectableList<Box<dyn Display>>,
    stage: Stage,
    input_buffer: Rc<RefCell<String>>,
    popup_content: Option<PopupInfo>,
    tx: &'a mpsc::UnboundedSender<BackendMessage>,
}

impl <'a> RunnersGroupsTab<'a> {
    pub fn new(groups: Vec<RunnerGroup>, tx: &'a mpsc::UnboundedSender<BackendMessage>) -> Self {
        let style = TODO_HEADER_STYLE.bg(Color::Green);
        RunnersGroupsTab {
            groups: FilterableList::new(groups, style).with_first_selected(),
            operations: SelectableList::new(GroupOperation::all(), style).with_first_selected(),
            stage: Stage::SelectGroup,
            dynamic_list: SelectableList::new(vec![], style),
            input_buffer: Rc::new(RefCell::new(String::new())),
            popup_content: None,
            tx
        }
    }

    pub fn toggle_loading(&mut self) {
        if let Some(popup) = &self.popup_content {
            if popup.is_loading {
                self.popup_content = None
            }
        }
    }

    pub fn set_groups(&mut self, groups: Vec<RunnerGroup>) {
        self.groups.items = groups.into_iter().map(|g|Rc::new(g)).collect();
        self.groups.filter_items();
        self.toggle_loading();
        self.stage = Stage::SelectGroup;
    }

    pub fn set_group_repos(&mut self, repos: Vec<ApiRepository>) {
        self.toggle_loading();
        let display_items = repos.into_iter()
            .map(|it|Box::new(it) as Box<dyn Display>)
            .collect();
        self.dynamic_list.set_items(display_items);
        self.stage = Stage::ListRepos;
    }

    pub fn selected(&self) -> Option<&RunnerGroup> {
        self.groups.selected()
    }

    fn add_to_input(&mut self, c: char) {
        self.input_buffer.borrow_mut().push(c);
    }

    fn remove_last_input(&mut self) {
        self.input_buffer.borrow_mut().pop();
    }

    fn drain_input(&mut self) -> String {
        std::mem::replace(&mut *self.input_buffer.borrow_mut(), String::new())
    }

    fn add_repo(&mut self) {
        self.popup_content = Some(PopupInfo::loading());
        let input = self.drain_input();
        let group = self.selected().unwrap();
        self.tx.send(BackendMessage::AddRepoToGroup(input, group.id))
            .expect("Could not send add repo command to backend");
        self.stage = Stage::SelectGroup;
    }

    fn get_repos(&mut self) {
        let group = self.selected().unwrap();
        self.tx.send(BackendMessage::GetGroupRepos(group.id))
            .expect("Could not send get group repos command to backend");
    }

    fn create_runner_group(&mut self) {
        let group = ApiRunnerGroupCreate {
            name: self.drain_input(),
            visibility: RunnerGroupVisibility::Selected,
            runners: vec![],
            selected_repository_ids: vec![],
        };
        self.tx.send(BackendMessage::CreateRunnerGroup(Box::new(group)))
            .expect("Could not send create runner command to backend");
        self.stage = Stage::SelectGroup;
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        match self.stage {
            Stage::SelectGroup => {
                let list_title = String::from("Runner Groups");
                self.groups.render(area, buf, &list_title);
            }
            Stage::SelectOperation | Stage::AddRepo => {
                let group = self.selected().unwrap();
                let list_title = format!("Select operation - {}", group.name);
                self.operations.render(area, buf, &list_title);
            }
            Stage::CreateGroup => {}
            Stage::ListRepos => {
                let group = self.selected().unwrap();
                let list_title = format!("Repos with access to group - {}", group.name);
                self.dynamic_list.render(area, buf, &list_title);
            }
        }
        show_popup(&self.popup_content, area, buf);
    }

    pub fn handle_input(&mut self, event: KeyEvent) -> bool {
        if event.code == KeyCode::Esc && self.popup_content.is_none() {
            return true;
        }
        match self.stage {
            Stage::SelectGroup => {
                match event.code {
                    KeyCode::Left => self.groups.select_none(),
                    KeyCode::Down => self.groups.select_next(),
                    KeyCode::Up => self.groups.select_previous(),
                    KeyCode::Home => self.groups.select_first(),
                    KeyCode::End => self.groups.select_last(),
                    KeyCode::Right | KeyCode::Enter => self.stage = Stage::SelectOperation,
                    KeyCode::Backspace => self.groups.remove_last_input(),
                    KeyCode::Char(c) => self.groups.update_filter(c),
                    _ => {}
                }
            }
            Stage::SelectOperation => {
                match event.code {
                    KeyCode::Up => self.operations.select_previous(),
                    KeyCode::Down => self.operations.select_next(),
                    KeyCode::Left => self.stage = Stage::SelectGroup,
                    KeyCode::Char(c) => match self.popup_content {
                        Some(_) => self.add_to_input(c),
                        _ => {}
                    }
                    KeyCode::Backspace => self.remove_last_input(),
                    KeyCode::Right | KeyCode::Enter => match self.operations.selected() {
                        Some(GroupOperation::AddRepo) => {
                            let input_clone = Rc::clone(&self.input_buffer);
                            self.popup_content = Some(
                                PopupInfo::new_dynamic(String::from("Input repo name:"),
                                                       Box::new(move ||format!("{}_", input_clone.borrow()))
                                ));
                            self.stage = Stage::AddRepo;
                        },
                        Some(GroupOperation::CreateGroup) => {
                            debug!("This should be anywhere else");
                            let input_clone = Rc::clone(&self.input_buffer);
                            self.popup_content = Some(
                                PopupInfo::new_dynamic(String::from("Input group name:"),
                                                       Box::new(move ||format!("{}_", input_clone.borrow()))
                                ));
                            self.stage = Stage::CreateGroup;
                        },
                        Some(GroupOperation::GetRepos) => {
                            self.get_repos();
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
            Stage::AddRepo => {
                match event.code {
                    KeyCode::Enter => self.add_repo(),
                    KeyCode::Esc => {
                        self.popup_content = None;
                        self.stage = Stage::SelectOperation;
                    }
                    KeyCode::Char(c) => self.add_to_input(c),
                    KeyCode::Backspace => self.remove_last_input(),
                    _ => {}
                }
            }
            Stage::ListRepos => {
                match event.code {
                    KeyCode::Left => self.stage = Stage::SelectOperation,
                    _ => {}
                }
            }
            Stage::CreateGroup => {
                match event.code {
                    KeyCode::Enter => self.create_runner_group(),
                    KeyCode::Esc => {
                        self.popup_content = None;
                        self.stage = Stage::SelectOperation;
                    }
                    KeyCode::Char(c) => self.add_to_input(c),
                    KeyCode::Backspace => self.remove_last_input(),
                    _ => {}
                }
            }
        }

        false
    }
}