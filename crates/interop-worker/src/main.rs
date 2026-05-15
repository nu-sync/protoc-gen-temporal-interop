#[allow(unused_imports)]
use futures_util::FutureExt as _;

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use interop_proto::TypedProtoMessage;
use interop_proto::interop::v1::{FinishRequest, RunRequest, RunResponse, Status};
use interop_proto::interop_v1_interop_service_temporal as temporal_contract;
use temporalio_client::{Client, ClientOptions, Connection, ConnectionOptions};
#[allow(unused_imports)]
use temporalio_macros::{init, query, run, signal, workflow, workflow_methods};
use temporalio_sdk::{
    SyncWorkflowContext, Worker, WorkerOptions, WorkflowContext, WorkflowContextView,
    WorkflowResult,
};
use temporalio_sdk_core::{CoreRuntime, RuntimeOptions, Url};

#[derive(Debug, Parser)]
struct Args {
    #[arg(long, default_value = "127.0.0.1:7233")]
    target_address: String,
    #[arg(long, default_value = "default")]
    namespace: String,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,interop_worker=debug".into()),
        )
        .try_init();

    let args = Args::parse();
    let temporal_url = normalize_temporal_url(&args.target_address);
    let runtime = CoreRuntime::new_assume_tokio(RuntimeOptions::default())
        .context("create Temporal core runtime")?;
    let connection = Connection::connect(
        ConnectionOptions::new(Url::parse(&temporal_url).context("parse Temporal URL")?).build(),
    )
    .await
    .context("connect to Temporal frontend")?;
    let client = Client::new(connection, ClientOptions::new(args.namespace).build())
        .context("build Temporal SDK client")?;

    let mut worker = Worker::new(&runtime, client, WorkerOptions::new("interop").build())
        .map_err(|err| anyhow!(err.to_string()))
        .context("create worker")?;
    temporal_contract::register_run_workflow::<InteropWorkflow>(&mut worker);

    tracing::info!(task_queue = "interop", "interop worker polling");
    worker.run().await.context("run worker")
}

fn normalize_temporal_url(target_address: &str) -> String {
    if target_address.starts_with("http://") || target_address.starts_with("https://") {
        target_address.to_string()
    } else {
        format!("http://{target_address}")
    }
}

#[workflow]
pub(crate) struct InteropWorkflow {
    input: RunRequest,
    stage: &'static str,
    finish_reason: Option<String>,
}

#[allow(dead_code)]
#[workflow_methods]
impl InteropWorkflow {
    #[init]
    fn new(_ctx: &WorkflowContextView, input: TypedProtoMessage<RunRequest>) -> Self {
        Self {
            input: input.into_inner(),
            stage: "started",
            finish_reason: None,
        }
    }

    #[run(name = temporal_contract::RUN_WORKFLOW_NAME)]
    async fn run(
        ctx: &mut WorkflowContext<Self>,
    ) -> WorkflowResult<TypedProtoMessage<RunResponse>> {
        ctx.wait_condition(|state| state.finish_reason.is_some())
            .await;
        ctx.state_mut(|state| {
            state.stage = "finished";
        });

        let response = ctx.state(|state| RunResponse {
            case_id: state.input.case_id.clone(),
            customer_id: state.input.customer_id.clone(),
            finish_reason: state.finish_reason.clone().unwrap_or_default(),
            observed_stage: state.stage.to_string(),
        });
        Ok(TypedProtoMessage(response))
    }

    #[signal(name = temporal_contract::FINISH_SIGNAL_NAME)]
    fn finish(
        &mut self,
        _ctx: &mut SyncWorkflowContext<Self>,
        input: TypedProtoMessage<FinishRequest>,
    ) {
        self.finish_reason = Some(input.into_inner().reason);
    }

    #[query(name = temporal_contract::GET_STATUS_QUERY_NAME)]
    fn get_status(&self, _ctx: &WorkflowContextView) -> TypedProtoMessage<Status> {
        TypedProtoMessage(Status {
            stage: self.stage.to_string(),
            case_id: self.input.case_id.clone(),
        })
    }
}

impl temporal_contract::RunDefinition for InteropWorkflow {
    type Input = RunRequest;
    type Output = RunResponse;
}
