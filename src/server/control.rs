use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use names::Generator;
use tokio::sync::mpsc;
use tonic::{Request, Response};

use crate::{
    api::{
        objects::v1alpha1::Service,
        superviseur::v1alpha1::{
            control_service_server::ControlService, ListRequest, ListResponse,
            ListRunningProcessesRequest, ListRunningProcessesResponse, LoadConfigRequest,
            LoadConfigResponse, RestartRequest, RestartResponse, StartRequest, StartResponse,
            StatusRequest, StatusResponse, StopRequest, StopResponse,
        },
    },
    graphql::{
        self,
        schema::objects::subscriptions::{
            AllServicesRestarted, AllServicesStarted, AllServicesStopped,
        },
        simple_broker::SimpleBroker,
    },
    superviseur::{ProcessEvent, Superviseur, SuperviseurCommand},
    types::{
        self,
        configuration::ConfigurationData,
        process::{Process, State},
    },
};

pub struct Control {
    cmd_tx: mpsc::UnboundedSender<SuperviseurCommand>,
    event_tx: mpsc::UnboundedSender<ProcessEvent>,
    superviseur: Superviseur,
    processes: Arc<Mutex<Vec<(Process, String)>>>,
    config_map: Arc<Mutex<HashMap<String, ConfigurationData>>>,
}

impl Control {
    pub fn new(
        cmd_tx: mpsc::UnboundedSender<SuperviseurCommand>,
        event_tx: mpsc::UnboundedSender<ProcessEvent>,
        superviseur: Superviseur,
        processes: Arc<Mutex<Vec<(Process, String)>>>,
        config_map: Arc<Mutex<HashMap<String, ConfigurationData>>>,
    ) -> Self {
        Self {
            cmd_tx,
            event_tx,
            superviseur,
            processes,
            config_map,
        }
    }
}

#[tonic::async_trait]
impl ControlService for Control {
    async fn load_config(
        &self,
        request: Request<LoadConfigRequest>,
    ) -> Result<Response<LoadConfigResponse>, tonic::Status> {
        let request = request.into_inner();
        let config = request.config;
        let path = request.file_path;
        let mut config: ConfigurationData =
            hcl::from_str(&config).map_err(|e| tonic::Status::internal(e.to_string()))?;

        let mut generator = Generator::default();
        let mut config_map = self.config_map.lock().unwrap();

        // check if the config is already loaded
        if config_map.contains_key(&path) {
            // reuse the id of the services
            let old_config = config_map.get_mut(&path).unwrap();
            for service in &mut config.services {
                match old_config.services.iter().find(|s| s.name == service.name) {
                    Some(old_service) => {
                        service.id = old_service.id.clone();

                        // rewacth the directory if working_dir changed
                        if old_service.working_dir != service.working_dir {
                            self.cmd_tx
                                .send(SuperviseurCommand::WatchForChanges(
                                    service.working_dir.clone(),
                                    service.clone(),
                                    config.project.clone(),
                                ))
                                .unwrap();
                        }
                    }
                    None => {
                        service.id = Some(generator.next().unwrap());
                    }
                }
            }
            self.cmd_tx
                .send(SuperviseurCommand::LoadConfig(config.clone(), path.clone()))
                .unwrap();
        } else {
            config.services = config
                .services
                .into_iter()
                .map(|mut service| {
                    service.id = Some(generator.next().unwrap());
                    service
                })
                .collect();

            config_map.insert(path.clone(), config.clone());

            let services = config.services.clone();
            let project = config.project.clone();

            for service in services.into_iter() {
                self.cmd_tx
                    .send(SuperviseurCommand::WatchForChanges(
                        service.working_dir.clone(),
                        service,
                        project.clone(),
                    ))
                    .unwrap();
            }

            self.cmd_tx
                .send(SuperviseurCommand::LoadConfig(
                    config.clone(),
                    config.project.clone(),
                ))
                .unwrap();
        }

        let config = config_map.get_mut(&path).unwrap();

        let services = config.services.clone();
        let mut services = services.into_iter();

        // convert services dependencies to ids
        for service in &mut config.services {
            let mut dependencies = vec![];
            for dependency in &service.depends_on {
                match services.find(|s| s.name == *dependency) {
                    Some(service) => {
                        dependencies.push(service.id.clone().unwrap());
                    }
                    None => {
                        return Err(tonic::Status::not_found(format!(
                            "Service {} not found",
                            dependency
                        )));
                    }
                }
            }
            service.dependencies = dependencies;
        }

        let services = config.services.clone();

        for service in services.into_iter() {
            self.cmd_tx
                .send(SuperviseurCommand::Load(service, config.project.clone()))
                .map_err(|e| tonic::Status::internal(e.to_string()))?;
        }

        thread::sleep(Duration::from_millis(100));

        Ok(Response::new(LoadConfigResponse { success: true }))
    }

    async fn start(
        &self,
        request: Request<StartRequest>,
    ) -> Result<Response<StartResponse>, tonic::Status> {
        let request = request.into_inner();
        let path = request.config_file_path;
        let name = request.name;
        let config_map = self.config_map.lock().unwrap();

        if !config_map.contains_key(&path) {
            return Err(tonic::Status::not_found("Config file not found"));
        }

        let config = config_map.get(&path).unwrap();

        if name.len() > 0 {
            let service = config
                .services
                .iter()
                .find(|s| s.name == name)
                .ok_or_else(|| tonic::Status::not_found("Service not found"))?;

            self.cmd_tx
                .send(SuperviseurCommand::Start(
                    service.clone(),
                    config.project.clone(),
                ))
                .map_err(|e| tonic::Status::internal(e.to_string()))?;
            return Ok(Response::new(StartResponse { success: true }));
        }

        for service in &config.services {
            self.cmd_tx
                .send(SuperviseurCommand::Start(
                    service.clone(),
                    config.project.clone(),
                ))
                .map_err(|e| tonic::Status::internal(e.to_string()))?;
        }

        let services = config.services.clone();
        let services = services
            .iter()
            .map(graphql::schema::objects::service::Service::from)
            .collect::<Vec<graphql::schema::objects::service::Service>>();
        SimpleBroker::publish(AllServicesStarted { payload: services });

        Ok(Response::new(StartResponse { success: true }))
    }

    async fn stop(
        &self,
        request: Request<StopRequest>,
    ) -> Result<Response<StopResponse>, tonic::Status> {
        let request = request.into_inner();
        let path = request.config_file_path;
        let name = request.name;
        let config_map = self.config_map.lock().unwrap();

        if !config_map.contains_key(&path) {
            return Err(tonic::Status::not_found("Config file not found"));
        }

        let config = config_map.get(&path).unwrap();

        if name.len() > 0 {
            let service = config
                .services
                .iter()
                .find(|s| s.name == name)
                .ok_or_else(|| tonic::Status::not_found("Service not found"))?;

            self.cmd_tx
                .send(SuperviseurCommand::Stop(
                    service.clone(),
                    config.project.clone(),
                ))
                .unwrap();
            return Ok(Response::new(StopResponse { success: true }));
        }

        for service in &config.services {
            self.cmd_tx
                .send(SuperviseurCommand::Stop(
                    service.clone(),
                    config.project.clone(),
                ))
                .unwrap();
        }

        let services = config.services.clone();
        let services = services
            .iter()
            .map(graphql::schema::objects::service::Service::from)
            .collect::<Vec<graphql::schema::objects::service::Service>>();
        SimpleBroker::publish(AllServicesStopped { payload: services });

        Ok(Response::new(StopResponse { success: true }))
    }

    async fn restart(
        &self,
        request: Request<RestartRequest>,
    ) -> Result<Response<RestartResponse>, tonic::Status> {
        let request = request.into_inner();
        let path = request.config_file_path;
        let name = request.name;
        let config_map = self.config_map.lock().unwrap();

        if !config_map.contains_key(&path) {
            return Err(tonic::Status::not_found("Config file not found"));
        }

        let config = config_map.get(&path).unwrap();

        if name.len() > 0 {
            let service = config
                .services
                .iter()
                .find(|s| s.name == name)
                .ok_or_else(|| tonic::Status::not_found("Service not found"))?;

            self.cmd_tx
                .send(SuperviseurCommand::Restart(
                    service.clone(),
                    config.project.clone(),
                ))
                .map_err(|e| tonic::Status::internal(e.to_string()))?;
            return Ok(Response::new(RestartResponse { success: true }));
        }

        for service in &config.services {
            self.cmd_tx
                .send(SuperviseurCommand::Restart(
                    service.clone(),
                    config.project.clone(),
                ))
                .map_err(|e| tonic::Status::internal(e.to_string()))?;
        }

        let services = config.services.clone();
        let services = services
            .iter()
            .map(graphql::schema::objects::service::Service::from)
            .collect::<Vec<graphql::schema::objects::service::Service>>();
        SimpleBroker::publish(AllServicesRestarted { payload: services });

        Ok(Response::new(RestartResponse { success: true }))
    }

    async fn status(
        &self,
        request: Request<StatusRequest>,
    ) -> Result<Response<StatusResponse>, tonic::Status> {
        let request = request.into_inner();
        let path = request.config_file_path;
        let name = request.name;
        let config_map = self.config_map.lock().unwrap();

        if !config_map.contains_key(&path) {
            return Err(tonic::Status::not_found("Config file not found"));
        }

        let config = config_map.get(&path).unwrap();

        let service = config
            .services
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(|| tonic::Status::not_found("Service not found"))?;

        let processes = self.processes.lock().unwrap();
        let process = processes
            .iter()
            .find(|(p, _)| p.name == name && p.project == config.project)
            .map(|(p, _)| p.clone())
            .unwrap_or(Process {
                name: name.clone(),
                project: config.project.clone(),
                r#type: service.r#type.clone(),
                state: types::process::State::Stopped,
                command: service.command.clone(),
                description: service.description.clone(),
                working_dir: service.working_dir.clone(),
                env: service.env.clone(),
                auto_restart: service.autorestart,
                stdout: service.stdout.clone(),
                stderr: service.stderr.clone(),
                ..Default::default()
            });
        Ok(Response::new(StatusResponse {
            process: Some(process.into()),
        }))
    }

    async fn list(
        &self,
        request: Request<ListRequest>,
    ) -> Result<Response<ListResponse>, tonic::Status> {
        let request = request.into_inner();
        let path = request.config_file_path;
        let config_map = self.config_map.lock().unwrap();

        if !config_map.contains_key(&path) {
            return Err(tonic::Status::not_found("Config file not found"));
        }

        let config = config_map.get(&path).unwrap();
        let services = config.services.clone();
        let mut list_response = ListResponse {
            services: services.into_iter().map(Service::from).collect(),
        };

        let processes = self.processes.lock().unwrap();
        for service in list_response.services.iter_mut() {
            let process = processes
                .iter()
                .find(|(p, _)| p.name == service.name)
                .map(|(p, _)| p);
            if let Some(process) = process {
                service.status = process.state.to_string().to_uppercase();
            } else {
                service.status = "STOPPED".to_string();
            }
        }

        Ok(Response::new(list_response))
    }

    async fn list_running_processes(
        &self,
        _request: Request<ListRunningProcessesRequest>,
    ) -> Result<Response<ListRunningProcessesResponse>, tonic::Status> {
        let processes = self.processes.lock().unwrap();
        let list_response = ListRunningProcessesResponse {
            processes: processes
                .iter()
                .filter(|(p, _)| p.state == State::Running)
                .map(|(p, _)| Into::into(p.clone()))
                .collect(),
        };
        Ok(Response::new(list_response))
    }
}
