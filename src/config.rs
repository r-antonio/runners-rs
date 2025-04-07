use std::collections::HashMap;
use std::fs;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub organization: String,
    pub token: String,
}

pub fn read_dot_env() -> Option<Config> {
    let contents = fs::read_to_string(".env")
        .expect("Something went wrong reading .env file");
    let attributes = contents.split("\n");
    let mut props = HashMap::<String, String>::new();
    attributes
        .map(|a| a.split_once("="))
        .for_each(|field| {
            match field {
                Some((key, value)) => props.insert(key.trim().to_string(), value.trim().to_string()),
                None => None
            };
        });
    Some(Config {
        organization: props.get("organization")?.to_string(),
        token: props.get("token")?.to_string(),
    })

}