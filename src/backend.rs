use crate::api::Client;
use crate::config::Config;
use crate::runners::{Runner, RunnerGroup};
use cli_log::debug;
use reqwest::header::{HeaderMap, HeaderValue};
use std::sync::Arc;
use tokio::sync::mpsc;

pub enum BackendMessage {
    FetchRunners,
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

    pub async fn get_runners(&mut self) -> Vec<Runner> {
        let groups_api = self.client.runner_groups().get_all().await.unwrap();
        let group_ids: Vec<(usize, String)> = groups_api.runner_groups.iter().map(|g| (g.id, g.name.clone())).collect();
        let groups = groups_api.runner_groups
            .into_iter()
            .map(|g|RunnerGroup::from(g))
            .collect();
        self.tx.send(ApiMessage::RunnerGroupList(groups))
            .expect("Could not sent command to backend worker");
        let futures = group_ids
            .into_iter()
            .map(|(id, name)| {
                let client_clone = Arc::clone(&self.client);
                async move {
                    let runners_api = client_clone.runner_groups().get_runners(id).await.unwrap().runners;
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

    pub async fn run(&mut self) {
        while let Some(message) = self.rx.recv().await {
            match message {
                    BackendMessage::FetchRunners => {
                        let runners = self.get_runners().await;
                        self.tx.send(ApiMessage::RunnerList(runners))
                            .expect("Could not send runner list to ui");
                    }
                    BackendMessage::AddLabel(runner_id, label) => {
                        println!("Updating label: {} for runner: {}", label, runner_id);
                    }
                    BackendMessage::DeleteLabel(runner_id, label) => {
                        todo!()
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