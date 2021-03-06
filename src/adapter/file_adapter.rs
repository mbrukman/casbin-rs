use crate::{
    adapter::{Adapter, Filter},
    error::ModelError,
    model::Model,
    Result,
};

#[cfg(feature = "runtime-async-std")]
use async_std::{
    fs::File,
    io::prelude::*,
    io::{BufReader, Error as IoError, ErrorKind},
    path::Path,
    prelude::*,
};

#[cfg(feature = "runtime-tokio")]
use std::{
    io::{Error as IoError, ErrorKind},
    path::Path,
};
#[cfg(feature = "runtime-tokio")]
use tokio::{
    fs::File,
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    stream::StreamExt,
};

use async_trait::async_trait;

use std::convert::AsRef;

pub struct FileAdapter<P> {
    file_path: P,
    is_filtered: bool,
}

type LoadPolicyFileHandler = fn(String, &mut dyn Model);
type LoadFilteredPolicyFileHandler = fn(String, &mut dyn Model, f: &Filter) -> bool;

impl<P> FileAdapter<P>
where
    P: AsRef<Path> + Send + Sync,
{
    pub fn new(p: P) -> FileAdapter<P> {
        FileAdapter {
            file_path: p,
            is_filtered: false,
        }
    }

    async fn load_policy_file(
        &self,
        m: &mut dyn Model,
        handler: LoadPolicyFileHandler,
    ) -> Result<()> {
        let f = File::open(&self.file_path).await?;
        let mut lines = BufReader::new(f).lines();

        while let Some(line) = lines.next().await {
            handler(line?, m);
        }
        Ok(())
    }

    async fn load_filtered_policy_file(
        &self,
        m: &mut dyn Model,
        filter: Filter,
        handler: LoadFilteredPolicyFileHandler,
    ) -> Result<bool> {
        let f = File::open(&self.file_path).await?;
        let mut lines = BufReader::new(f).lines();

        let mut is_filtered = false;
        while let Some(line) = lines.next().await {
            if handler(line?, m, &filter) {
                is_filtered = true;
            }
        }
        Ok(is_filtered)
    }

    async fn save_policy_file(&self, text: String) -> Result<()> {
        let mut file = File::create(&self.file_path).await?;
        file.write_all(text.as_bytes()).await?;
        Ok(())
    }
}

#[async_trait]
impl<P> Adapter for FileAdapter<P>
where
    P: AsRef<Path> + Send + Sync,
{
    async fn load_policy(&self, m: &mut dyn Model) -> Result<()> {
        self.load_policy_file(m, load_policy_line).await?;
        Ok(())
    }

    async fn load_filtered_policy(&mut self, m: &mut dyn Model, f: Filter) -> Result<()> {
        if self
            .load_filtered_policy_file(m, f, load_filtered_policy_line)
            .await?
        {
            self.is_filtered = true;
        }
        Ok(())
    }

    async fn save_policy(&mut self, m: &mut dyn Model) -> Result<()> {
        if self.file_path.as_ref().as_os_str().is_empty() {
            return Err(
                IoError::new(ErrorKind::Other, "save policy failed, file path is empty").into(),
            );
        }

        let mut tmp = String::new();
        let ast_map1 = m
            .get_model()
            .get("p")
            .ok_or_else(|| ModelError::P("Missing policy definition in conf file".to_owned()))?;

        for (ptype, ast) in ast_map1 {
            for rule in ast.get_policy() {
                let s1 = format!("{}, {}\n", ptype, rule.join(","));
                tmp += s1.as_str();
            }
        }

        if let Some(ast_map2) = m.get_model().get("g") {
            for (ptype, ast) in ast_map2 {
                for rule in ast.get_policy() {
                    let s1 = format!("{}, {}\n", ptype, rule.join(","));
                    tmp += s1.as_str();
                }
            }
        }

        self.save_policy_file(tmp).await?;
        Ok(())
    }

    async fn add_policy(&mut self, _sec: &str, _ptype: &str, _rule: Vec<String>) -> Result<bool> {
        // this api shouldn't implement, just for convenience
        Ok(true)
    }

    async fn add_policies(
        &mut self,
        _sec: &str,
        _ptype: &str,
        _rules: Vec<Vec<String>>,
    ) -> Result<bool> {
        // this api shouldn't implement, just for convenience
        Ok(true)
    }

    async fn remove_policy(
        &mut self,
        _sec: &str,
        _ptype: &str,
        _rule: Vec<String>,
    ) -> Result<bool> {
        // this api shouldn't implement, just for convenience
        Ok(true)
    }

    async fn remove_policies(
        &mut self,
        _sec: &str,
        _ptype: &str,
        _rule: Vec<Vec<String>>,
    ) -> Result<bool> {
        // this api shouldn't implement, just for convenience
        Ok(true)
    }

    async fn remove_filtered_policy(
        &mut self,
        _sec: &str,
        _ptype: &str,
        _field_index: usize,
        _field_values: Vec<String>,
    ) -> Result<bool> {
        // this api shouldn't implement, just for convenience
        Ok(true)
    }

    fn is_filtered(&self) -> bool {
        self.is_filtered
    }
}

fn load_policy_line(line: String, m: &mut dyn Model) {
    if line.is_empty() || line.starts_with('#') {
        return;
    }
    let tokens: Vec<String> = line.split(',').map(|x| x.trim().to_string()).collect();
    let key = tokens[0].clone();

    if let Some(sec) = key.chars().next().map(|x| x.to_string()) {
        if let Some(t1) = m.get_mut_model().get_mut(&sec) {
            if let Some(t2) = t1.get_mut(&key) {
                t2.policy.insert(tokens[1..].to_vec());
            }
        }
    }
}

fn load_filtered_policy_line(line: String, m: &mut dyn Model, f: &Filter) -> bool {
    if line.is_empty() || line.starts_with('#') {
        return false;
    }
    let tokens: Vec<String> = line.split(',').map(|x| x.trim().to_string()).collect();
    let key = tokens[0].clone();

    let mut is_filtered = false;
    if let Some(sec) = key.chars().next().map(|x| x.to_string()) {
        if &sec == "p" {
            for (i, rule) in f.p.iter().enumerate() {
                if !rule.is_empty() && rule != &tokens[i + 1] {
                    is_filtered = true;
                }
            }
        }
        if &sec == "g" {
            for (i, rule) in f.g.iter().enumerate() {
                if !rule.is_empty() && rule != &tokens[i + 1] {
                    is_filtered = true;
                }
            }
        }
        if !is_filtered {
            if let Some(t1) = m.get_mut_model().get_mut(&sec) {
                if let Some(t2) = t1.get_mut(&key) {
                    t2.policy.insert(tokens[1..].to_vec());
                }
            }
        }
    }

    is_filtered
}
