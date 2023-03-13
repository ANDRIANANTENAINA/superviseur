use async_graphql::{Context, Object, ID};

use super::objects::{process::Process, service::Service};

#[derive(Default, Clone)]
pub struct ControlQuery;

#[Object]
impl ControlQuery {
    async fn status(&self, ctx: &Context<'_>, id: ID) -> Process {
        Process {
            name: "test".to_string(),
            description: "test".to_string(),
            pid: 1,
            ppid: 1,
            command: "test".to_string(),
            working_directory: "test".to_string(),
            project: "test".to_string(),
            r#type: "test".to_string(),
            log_file: "test".to_string(),
            stderr_file: "test".to_string(),
            auto_restart: true,
            env: vec!["test".to_string()],
        }
    }

    async fn services(&self, ctx: &Context<'_>) -> Vec<Service> {
        vec![]
    }

    async fn processes(&self, ctx: &Context<'_>) -> Vec<Process> {
        vec![]
    }

    async fn service(&self, ctx: &Context<'_>, id: ID) -> Service {
        Service {
            id: ID("test".to_string()),
            name: "test".to_string(),
            command: "test".to_string(),
            description: "test".to_string(),
            namespace: "test".to_string(),
            r#type: "test".to_string(),
            status: "test".to_string(),
            depends_on: vec!["test".to_string()],
            env: vec!["test".to_string()],
            auto_restart: true,
            working_directory: "test".to_string(),
            log_file: "test".to_string(),
            stderr_file: "test".to_string(),
            port: 1,
        }
    }
}

#[derive(Default, Clone)]
pub struct ControlMutation;

#[Object]
impl ControlMutation {
    async fn start(&self, ctx: &Context<'_>, id: ID) -> Process {
        Process {
            name: "test".to_string(),
            description: "test".to_string(),
            pid: 1,
            ppid: 1,
            command: "test".to_string(),
            working_directory: "test".to_string(),
            project: "test".to_string(),
            r#type: "test".to_string(),
            log_file: "test".to_string(),
            stderr_file: "test".to_string(),
            auto_restart: true,
            env: vec!["test".to_string()],
        }
    }

    async fn stop(&self, ctx: &Context<'_>, id: ID) -> Process {
        Process {
            name: "test".to_string(),
            description: "test".to_string(),
            pid: 1,
            ppid: 1,
            command: "test".to_string(),
            working_directory: "test".to_string(),
            project: "test".to_string(),
            r#type: "test".to_string(),
            log_file: "test".to_string(),
            stderr_file: "test".to_string(),
            auto_restart: true,
            env: vec!["test".to_string()],
        }
    }

    async fn restart(&self, ctx: &Context<'_>, id: ID) -> Process {
        Process {
            name: "test".to_string(),
            description: "test".to_string(),
            pid: 1,
            ppid: 1,
            command: "test".to_string(),
            working_directory: "test".to_string(),
            project: "test".to_string(),
            r#type: "test".to_string(),
            log_file: "test".to_string(),
            stderr_file: "test".to_string(),
            auto_restart: true,
            env: vec!["test".to_string()],
        }
    }
}
