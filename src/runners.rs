use std::fmt::Display;
use ratatui::prelude::Line;
use ratatui::widgets::ListItem;
use crate::api::{ApiRunner, ApiRunnerGroup, RunnerGroupVisibility};
use crate::{COMPLETED_TEXT_FG_COLOR, TEXT_FG_COLOR};

#[derive(Debug, Clone)]
pub enum RunnerStatus {
    Online,
    Offline,
    Busy,
}

#[derive(Debug, Clone)]
pub struct Runner {
    pub id: usize,
    pub status: RunnerStatus,
    pub name: String,
    pub labels: Vec<String>,
    pub group: Option<String>,
}

impl Display for Runner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} - {} - {}", self.name.to_string(), self.id, self.labels.join("|"))
    }
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
            runner.labels.iter().filter(|label| label.label_type == "custom").map(|x| x.name.to_string()).collect(),
            None
        )
    }
}

impl From<&Runner> for ListItem<'_> {
    fn from(value: &Runner) -> Self {
        let group_name = if let Some(group) = &value.group { group } else { &"default".to_string()};
        let labels = value.labels.join(" | ");
        let text = format!("{} ({}) | {}", &value.name, &group_name, &labels);
        let line = match value.status {
            RunnerStatus::Online => Line::styled(format!(" ✓ {}", &text), TEXT_FG_COLOR),
            RunnerStatus::Offline => {
                Line::styled(format!(" x {}", &text), COMPLETED_TEXT_FG_COLOR)
            }
            RunnerStatus::Busy => Line::styled(format!(" ☐ {}", &text), TEXT_FG_COLOR)
        };
        ListItem::new(line)
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

pub struct RunnerGroup {
    pub id: usize,
    pub name: String,
    pub visibility: RunnerGroupVisibility,
}

impl Display for RunnerGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name.to_string())
    }
}

impl RunnerGroup {
    fn new(id: usize, name: String, visibility: RunnerGroupVisibility) -> Self {
        RunnerGroup {
            id, name, visibility
        }
    }
}

impl From<&RunnerGroup> for ListItem<'_> {
    fn from(value: &RunnerGroup) -> Self {
        let text = format!("{} ID: {}", &value.name, &value.id);
        let line = Line::styled(format!(" {}", &text), TEXT_FG_COLOR);
        ListItem::new(line)
    }
}

impl From<ApiRunnerGroup> for RunnerGroup {
    fn from(group: ApiRunnerGroup) -> Self {
        RunnerGroup::new(
            group.id,
            group.name,
            group.visibility
        )
    }
}