use crate::client::api::{ApiRepository, ApiRunnerGroupCreate, Client, RunnerGroupVisibility};
use crate::model::runners::{Runner, RunnerGroup};
use crate::utils::config::Config;
use cli_log::debug;
use reqwest::header::{HeaderMap, HeaderValue};
use std::sync::Arc;
use tokio::sync::mpsc;

pub enum BackendMessage {
    FetchRunners,
    FetchGroups,
    AddLabel(usize, String),
    DeleteLabel(usize, String),
    ChangeGroup(usize, String),
    AddRepoToGroup(String, usize),
    GetGroupRepos(usize),
    CreateRunnerGroup(Box<ApiRunnerGroupCreate>),
}

pub enum ApiMessage {
    Ok,
    RunnerList(Box<Vec<Runner>>),
    RunnerGroupList(Box<Vec<RunnerGroup>>),
    GroupRepos(Box<Vec<ApiRepository>>)
}

pub struct Worker {
    pub client: Arc<Client>,
    pub config: Config,
    pub rx: mpsc::UnboundedReceiver<BackendMessage>,
    pub tx: mpsc::UnboundedSender<ApiMessage>,
}

impl Worker {
    pub fn new(rx: mpsc::UnboundedReceiver<BackendMessage>, tx: mpsc::UnboundedSender<ApiMessage>, config: Config) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert("User-Agent", HeaderValue::from_str("curl").unwrap());
        headers.insert("Authorization", HeaderValue::from_str(&format!("Bearer {}", config.token)).unwrap());
        let github_client = Client::new(&format!("https://api.github.com/orgs/{}/", config.organization), headers)
            .expect("Failed to create github client");
        let client = Arc::new(github_client);
        Worker { client, rx, tx, config }
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
        self.tx.send(ApiMessage::RunnerGroupList(Box::new(groups)))
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
        self.tx.send(ApiMessage::RunnerList(Box::new(runners)))
            .expect("Could not send refreshed runner list to frontend");
    }

    pub async fn run(&mut self) {
        while let Some(message) = self.rx.recv().await {
            match message {
                    BackendMessage::FetchGroups => {
                        let groups = self.get_runner_groups().await;
                        self.tx.send(ApiMessage::RunnerGroupList(Box::new(groups)))
                            .expect("Could not sent command to frontend worker");
                    }
                    BackendMessage::FetchRunners => {
                        let runners = self.get_runners(None).await;
                        self.tx.send(ApiMessage::RunnerList(Box::new(runners)))
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
                        self.client.runners().remove_label(runner_id, label).await
                            .expect("Could not remove label");
                        self.refresh_runners().await;
                    }
                    BackendMessage::ChangeGroup(runner_id, group_name) => {
                        debug!("Changing group of runner {} to group {}", runner_id, group_name);
                        let group = match self.client.runner_groups().get_all(false).await {
                            Ok(response) => response.runner_groups.into_iter().find(|r|r.name == group_name).unwrap(),
                            Err(e) => panic!("Error getting runner group {}: {}", group_name, e),
                        };
                        self.client.runner_groups().add_runner_to_group(runner_id, group.id).await
                            .expect("Could not add runner to group");
                        self.refresh_runners().await;
                    }
                    BackendMessage::AddRepoToGroup(repo_name, group_id) => {
                        debug!("Adding repo {} to group id {}", repo_name, group_id);
                        let repo = self.client.repos().get_repo(&self.config.organization, &repo_name).await
                            .expect("Could not get repo");
                        self.client.runner_groups().add_repo_access(group_id, repo.id).await
                            .expect("Could not add repo to group");
                        self.tx.send(ApiMessage::Ok)
                            .expect("Could not send response to frontend");
                    }
                    BackendMessage::CreateRunnerGroup(runner_group) => {
                        debug!("Creating runner group {:?}", runner_group);
                        self.client.runner_groups().create_runner_group(*runner_group).await
                            .expect("Could not create runner group");
                        self.refresh_runners().await;
                    },
                    BackendMessage::GetGroupRepos(runner_group_id) => {
                        debug!("Getting group repos {}", runner_group_id);
                        let result = self.client.runner_groups().get_group_repos(runner_group_id).await
                            .expect("Could not get group repos");
                        debug!("Fetched repos {:?}", result.repositories);
                        self.tx.send(ApiMessage::GroupRepos(Box::new(result.repositories)))
                            .expect("Could not send group repos response to frontend");
                    }
                }
            }
        }
}