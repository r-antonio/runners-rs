use anyhow::Result;
use reqwest::{Error, StatusCode, Url};
use std::sync::{Arc, RwLock};
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use tokio;
use cli_log::*;
use crate::RunnerGroup;

pub struct Client {
    api_base: Url,
    client: Arc<reqwest::Client>,
}

impl Client {
    pub fn new(api_base: &str, default_headers: HeaderMap) -> Result<Self> {
        let api_base = Url::parse(api_base)?;
        let client = Arc::new(reqwest::Client::builder()
            .default_headers(default_headers).build()?);
        Ok(Client { api_base, client })
    }

    pub fn runners(&self) -> RunnersEndpoint {
        RunnersEndpoint(self)
    }

    pub fn runner_groups(&self) -> RunnersGroupsEndpoint {
        RunnersGroupsEndpoint(self)
    }
}

pub struct RunnersEndpoint<'c>(&'c Client);

impl<'c> RunnersEndpoint<'c> {
    fn endpoint(&self) -> Result<Url> {
        Ok(self.0.api_base.join("runners")?)
    }

    pub async fn get_all(&self) -> Result<RunnersResponse> {
        let endpoint = self.endpoint()?;
        debug!("GET {}", endpoint);
        Ok(self.0.client.get(endpoint).send().await?.json::<RunnersResponse>().await?)
    }
}

#[derive(Debug, Deserialize)]
pub struct RunnersGroupResponse {
    total_count: usize,
    runner_groups: Vec<ApiRunnerGroup>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum RunnerGroupVisibility {
    #[serde(rename = "selected")]
    Selected,
    #[serde(rename = "all")]
    All,
}

#[derive(Debug, Deserialize)]
pub struct ApiRunnerGroup {
    id: usize,
    name: String,
    visibility: RunnerGroupVisibility,
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
    name: String,
    visibility: RunnerGroupVisibility,
    selected_repository_ids: Vec<usize>,
    runners: Vec<usize>,
}

pub struct RunnersGroupsEndpoint<'c>(&'c Client);
impl<'c> RunnersGroupsEndpoint<'c> {
    fn endpoint(&self) -> Result<Url> {
        Ok(self.0.api_base.join("runner-groups")?)
    }

    pub async fn get_all(&self) -> Result<RunnersGroupResponse> {
        let endpoint = self.endpoint()?;
        debug!("GET {}", endpoint);
        Ok(self.0.client.get(endpoint).send().await?.json::<RunnersGroupResponse>().await?)
    }

    pub async fn get_runners(&self, group_id: usize) -> Result<RunnersResponse> {
        let endpoint = self.endpoint()?.join(&format!("/{}/hosted-runners", group_id))?;
        debug!("GET {}", endpoint);
        Ok(self.0.client.get(endpoint).send().await?.json::<RunnersResponse>().await?)
    }

    pub async fn create_runner_group(&self, runner_group: ApiRunnerGroupCreate) -> Result<ApiRunnerGroup> {
        let endpoint = self.endpoint()?;
        debug!("POST {} : {:?}", endpoint, runner_group);
        Ok(self.0.client.post(endpoint).json(&runner_group).send().await?.json::<ApiRunnerGroup>().await?)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct APILabel {
    pub id: usize,
    pub name: String,
    #[serde(rename = "type")]
    pub label_type: String,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct ApiRunner {
    pub id: usize,
    pub name: String,
    pub os: String,
    pub status: String,
    pub busy: bool,
    pub ephemeral: Option<bool>,
    pub labels: Vec<APILabel>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RunnersResponse{
    pub total_count: usize,
    pub runners: Vec<ApiRunner>
}