use crate::api::{ApiRunner, ApiRunnerGroup, RunnerGroupVisibility};
use std::fmt::Display;
use std::str::FromStr;

#[derive(Debug, Clone)]
pub enum RunnerStatus {
    Online,
    Offline,
    Busy,
}

impl FromStr for RunnerStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "online" => Ok(RunnerStatus::Online),
            "offline" => Ok(RunnerStatus::Offline),
            "busy" => Ok(RunnerStatus::Busy),
            _ => Err(format!("Unknown runner status: {}", s)),
        }
    }
}

impl Display for RunnerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            RunnerStatus::Online => "online",
            RunnerStatus::Offline => "offline",
            RunnerStatus::Busy => "busy",
        };
        write!(f, "{}", value)
    }
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
        let group_name = if let Some(group) = &self.group { group } else { &"default".to_string()};
        let labels = self.labels.join(" | ");
        let text = format!("{} [{}] ({}) | {}", &self.name, &self.status, &group_name, &labels);
        write!(f, "{}", text)
    }
}

impl From<ApiRunner> for Runner {
    fn from(runner: ApiRunner) -> Self {
        let status = if runner.busy {
            RunnerStatus::Busy
        } else {
            RunnerStatus::from_str(&runner.status).unwrap()
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

#[derive(Clone)]
pub struct RunnerGroup {
    pub id: usize,
    pub name: String,
    pub visibility: RunnerGroupVisibility,
}

impl Display for RunnerGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ID: {}", self.name.to_string(), self.id)
    }
}

impl RunnerGroup {
    fn new(id: usize, name: String, visibility: RunnerGroupVisibility) -> Self {
        RunnerGroup {
            id, name, visibility
        }
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