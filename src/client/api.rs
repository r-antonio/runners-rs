use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
use anyhow::Result;
use cli_log::*;
use reqwest::header::HeaderMap;
use reqwest::{Url};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use crate::utils::cache::Cache;

pub struct Client {
    api_base: Url,
    client: Arc<reqwest::Client>,
    runners: Arc<Mutex<Cache<RunnersResponse>>>,
    runner_groups: Arc<Mutex<Cache<RunnersGroupResponse>>>,
}

impl Client {
    pub fn new(api_base: &str, default_headers: HeaderMap) -> Result<Self> {
        let api_base = Url::parse(api_base)?;
        let client = Arc::new(reqwest::Client::builder()
            .default_headers(default_headers).build()?);
        Ok(Client {
            api_base,
            client,
            runners: Arc::new(Mutex::new(Cache::new())),
            runner_groups: Arc::new(Mutex::new(Cache::new())) })
    }

    pub fn runners(&self) -> RunnersEndpoint {
        RunnersEndpoint(self)
    }

    pub fn runner_groups(&self) -> RunnersGroupsEndpoint {
        RunnersGroupsEndpoint(self)
    }

    pub fn repos(&self) -> RepoEndpoint {
        RepoEndpoint(self)
    }
}

trait CustomEndpoint {
    fn endpoint(&self, base_url: &Url, path: &str) -> Result<Url> {
        Ok(base_url.join(path)?)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LabelsBody {
    labels: Vec<String>
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiRepository {
    pub id: usize,
    pub name: String,
}

impl Display for ApiRepository {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[derive(Deserialize)]
pub struct ApiRepositoriesResponse {
    pub total_count: usize,
    pub repositories: Vec<ApiRepository>,
}

pub struct RepoEndpoint<'c>(&'c Client);
impl CustomEndpoint for RepoEndpoint<'_> {}

impl <'c> RepoEndpoint<'c> {
    pub async fn get_repo(&self, org: &str, repo: &str) -> Result<ApiRepository>{
        let endpoint = self.0.api_base.join(&format!("/repos/{}/{}", org, repo))?;
        debug!("GET {}", endpoint);
        Ok(self.0.client.get(endpoint).send().await?.json::<ApiRepository>().await?)
    }
}

pub struct RunnersEndpoint<'c>(&'c Client);

impl CustomEndpoint for RunnersEndpoint<'_> {}

impl<'c> RunnersEndpoint<'c> {
    pub async fn get_all(&self) -> Result<RunnersResponse> {
        let endpoint = self.endpoint(&self.0.api_base, "actions/runners")?;
        debug!("GET {}", endpoint);
        Ok(self.0.client.get(endpoint).send().await?.json::<RunnersResponse>().await?)
    }

    pub async fn add_label(&self, id: usize, labels: Vec<String>) -> Result<()> {
        let endpoint = self.endpoint(&self.0.api_base, &format!("actions/runners/{}/labels", id))?;
        debug!("POST {}", endpoint);
        let body = LabelsBody { labels };
        self.0.client.post(endpoint).json(&body).send().await?.error_for_status()?;
        Ok(())
    }

    pub async fn remove_label(&self, id: usize, label: String) -> Result<()> {
        let endpoint = self.endpoint(&self.0.api_base, &format!("actions/runners/{}/labels/{}", id, label))?;
        debug!("DELETE {}", endpoint);
        self.0.client.delete(endpoint).send().await?.error_for_status()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RunnersGroupResponse {
    pub total_count: usize,
    pub runner_groups: Vec<ApiRunnerGroup>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum RunnerGroupVisibility {
    #[serde(rename = "selected")]
    Selected,
    #[serde(rename = "all")]
    All,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ApiRunnerGroup {
    pub id: usize,
    pub name: String,
    pub visibility: RunnerGroupVisibility,
    default: bool,
    selected_repositories_url: Option<String>,
    runners_url: String,
    inherited: bool,
    allows_public_repositories: bool,
    restricted_to_workflows: bool,
    selected_workflows: Vec<String>,
    workflow_restrictions_read_only: bool,
}

#[derive(Debug, Serialize)]
pub struct ApiRunnerGroupCreate {
    pub name: String,
    pub visibility: RunnerGroupVisibility,
    pub selected_repository_ids: Vec<usize>,
    pub runners: Vec<usize>,
}

pub struct RunnersGroupsEndpoint<'c>(&'c Client);
impl CustomEndpoint for RunnersGroupsEndpoint<'_> {}
impl<'c> RunnersGroupsEndpoint<'c> {
    pub async fn get_all(&self, skip_cache: bool) -> Result<RunnersGroupResponse> {
        let endpoint = self.endpoint(&self.0.api_base, "actions/runner-groups")?;
        let key = endpoint.as_str().to_string();
        if !skip_cache {
            if let Some(result) = self.0.runner_groups.lock().unwrap().get(&key) {
                debug!("Cache hit: {}", endpoint);
                return Ok(result.clone());
            }
        }
        debug!("GET {}", endpoint);
        let response = self.0.client.get(endpoint).send().await?.json::<RunnersGroupResponse>().await?;
        let response_clone = response.clone();
        self.0.runner_groups.lock().unwrap().insert(key.to_string(), response);
        Ok(response_clone)
    }

    pub async fn get_runners(&self, group_id: usize, skip_cache: bool) -> Result<RunnersResponse> {
        let endpoint = self.endpoint(&self.0.api_base, &format!("actions/runner-groups/{}/runners", group_id))?;
        let key = endpoint.as_str().to_string();
        if !skip_cache {
            if let Some(result) = self.0.runners.lock().unwrap().get(&key) {
                debug!("Cache hit: {}", endpoint);
                return Ok(result.clone())
            }
        }
        debug!("GET {}", endpoint);
        let response = self.0.client.get(endpoint).send().await?.json::<RunnersResponse>().await?;
        let response_clone = response.clone();
        self.0.runners.lock().unwrap().insert(key.to_string(), response);
        Ok(response_clone)
    }

    pub async fn create_runner_group(&self, runner_group: ApiRunnerGroupCreate) -> Result<ApiRunnerGroup> {
        let endpoint = self.endpoint(&self.0.api_base, "actions/runner-groups")?;
        debug!("POST {} : {:?}", endpoint, runner_group);
        Ok(self.0.client.post(endpoint).json(&runner_group).send().await?.json::<ApiRunnerGroup>().await?)
    }

    pub async fn add_runner_to_group(&self, runner_id: usize, runner_group_id: usize) -> Result<()>{
        let endpoint = self.endpoint(&self.0.api_base, &format!("actions/runner-groups/{}/runners/{}", runner_group_id, runner_id))?;
        debug!("PUT {}", endpoint);
        self.0.client.put(endpoint).send().await?.error_for_status()?;
        Ok(())
    }

    pub async fn add_repo_access(&self, runner_group_id: usize, repo_id: usize) -> Result<()> {
        let endpoint = self.endpoint(&self.0.api_base, &format!("actions/runner-groups/{}/repositories/{}", runner_group_id, repo_id))?;
        debug!("PUT {}", endpoint);
        self.0.client.put(endpoint).send().await?.error_for_status()?;
        Ok(())
    }

    pub async fn get_group_repos(&self, runner_group_id: usize) -> Result<ApiRepositoriesResponse> {
        let endpoint = self.endpoint(&self.0.api_base, &format!("actions/runner-groups/{}/repositories", runner_group_id))?;
        debug!("GET {}", endpoint);
        Ok(self.0.client.get(endpoint).send().await?.json::<ApiRepositoriesResponse>().await?)
    }

}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct APILabel {
    pub id: usize,
    pub name: String,
    #[serde(rename = "type")]
    pub label_type: String,
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ApiRunner {
    pub id: usize,
    pub name: String,
    pub os: String,
    pub status: String,
    pub busy: bool,
    pub ephemeral: Option<bool>,
    pub labels: Vec<APILabel>,
    #[serde(skip_deserializing)]
    pub group_id: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RunnersResponse{
    pub total_count: usize,
    pub runners: Vec<ApiRunner>
}