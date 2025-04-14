use crate::api::Client;
use crate::config::Config;
use crate::runners::{Runner, RunnerGroup};
use cli_log::debug;
use reqwest::header::{HeaderMap, HeaderValue};
use std::sync::Arc;
use tokio::sync::mpsc;

pub enum BackendMessage {
    FetchRunners,
    FetchGroups,
    AddLabel(usize, String),
    DeleteLabel(usize, String),
    ChangeGroup(usize, usize),
    AddRepoToGroup(usize, usize),
}

pub enum ApiMessage {
    RunnerList(Vec<Runner>),
    RunnerGroupList(Vec<RunnerGroup>),
}

pub struct Worker {
    pub client: Arc<Client>,
    pub rx: mpsc::UnboundedReceiver<BackendMessage>,
    pub tx: mpsc::UnboundedSender<ApiMessage>,
}

impl Worker {
    pub fn new(rx: mpsc::UnboundedReceiver<BackendMessage>, tx: mpsc::UnboundedSender<ApiMessage>, config: &Config) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert("User-Agent", HeaderValue::from_str("curl").unwrap());
        headers.insert("Authorization", HeaderValue::from_str(&format!("Bearer {}", config.token)).unwrap());
        let github_client = Client::new(&format!("https://api.github.com/orgs/{}/actions/", config.organization), headers)
            .expect("Failed to create github client");
        let client = Arc::new(github_client);
        Worker { client, rx, tx }
    }

    pub async fn get_runner_groups(&mut self) -> Vec<RunnerGroup> {
        let groups_api = self.client.runner_groups().get_all(false).await.unwrap();
        groups_api.runner_groups
            .into_iter()
            .map(|g|RunnerGroup::from(g))
            .collect()
    }

    pub async fn get_runners(&mut self, skip_cache: Option<bool>) -> Vec<Runner> {
        let dirty = skip_cache.unwrap_or(false);
        let groups_api = self.client.runner_groups().get_all(dirty).await.unwrap();
        let group_ids: Vec<(usize, String)> = groups_api.runner_groups.iter().map(|g| (g.id, g.name.clone())).collect();
        let groups = groups_api.runner_groups
            .into_iter()
            .map(|g|RunnerGroup::from(g))
            .collect();
        self.tx.send(ApiMessage::RunnerGroupList(groups))
            .expect("Could not sent command to frontend worker");
        let futures = group_ids
            .into_iter()
            .map(|(id, name)| {
                let client_clone = Arc::clone(&self.client);
                async move {
                    let runners_api = client_clone.runner_groups().get_runners(id, dirty).await.unwrap().runners;
                    runners_api.into_iter().map(|r| {
                        let mut runner = Runner::from(r);
                        runner.group = Some(name.clone());
                        runner
                    }).collect()
                }
            } );
        let results: Vec<Vec<Runner>> = futures::future::join_all(futures).await;
        let runners: Vec<Runner> = results.into_iter()
            .flatten().collect();
        debug!("Fetched runners {:?}", runners);
        runners
    }

    pub async fn refresh_runners(&mut self) {
        let runners = self.get_runners(Some(true)).await;
        self.tx.send(ApiMessage::RunnerList(runners))
            .expect("Could not send refreshed runner list to frontend");
    }

    pub async fn run(&mut self) {
        while let Some(message) = self.rx.recv().await {
            match message {
                    BackendMessage::FetchGroups => {
                        let groups = self.get_runner_groups().await;
                        self.tx.send(ApiMessage::RunnerGroupList(groups))
                            .expect("Could not sent command to frontend worker");
                    }
                    BackendMessage::FetchRunners => {
                        let runners = self.get_runners(None).await;
                        self.tx.send(ApiMessage::RunnerList(runners))
                            .expect("Could not send runner list to ui");
                    }
                    BackendMessage::AddLabel(runner_id, label) => {
                        debug!("Updating label: {} for runner: {}", label, runner_id);
                        let labels = vec![label];
                        self.client.runners().add_label(runner_id, labels).await
                            .expect("Could not add label");
                        self.refresh_runners().await;
                    }
                    BackendMessage::DeleteLabel(runner_id, label) => {
                        debug!("Removing label: {} for runner {}", label, runner_id);
                    }
                    BackendMessage::ChangeGroup(runner_id, group_id) => {
                        todo!()
                    }
                    BackendMessage::AddRepoToGroup(repo_id, group_id) => {
                        todo!()
                    }
                }
            }
        }
}