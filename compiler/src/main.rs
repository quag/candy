#![feature(async_closure)]
#![feature(box_patterns)]
#![feature(let_chains)]
#![feature(never_type)]
#![feature(try_trait_v2)]
#![allow(clippy::module_inception)]

mod builtin_functions;
mod compiler;
mod database;
mod fuzzer;
mod language_server;
mod module;
mod vm;

use crate::{
    compiler::{
        ast_to_hir::AstToHir,
        cst_to_ast::CstToAst,
        error::CompilerError,
        hir::{self, CollectErrors, Id},
        hir_to_lir::HirToLir,
        rcst_to_cst::RcstToCst,
        string_to_rcst::StringToRcst,
    },
    database::Database,
    language_server::utils::LspPositionConversion,
    module::{Module, ModuleKind},
    vm::{
        context::{DbUseProvider, RunForever},
        tracer::{FullTracer, Tracer},
        Closure, ExecutionResult, FiberId, Status, Struct, Vm,
    },
};
use compiler::lir::Lir;
use itertools::Itertools;
use language_server::CandyLanguageServer;
use notify::{watcher, RecursiveMode, Watcher};
use std::{
    collections::HashMap,
    convert::TryInto,
    env::current_dir,
    path::PathBuf,
    sync::{mpsc::channel, Arc},
    time::Duration,
};
use structopt::StructOpt;
use tower_lsp::{LspService, Server};
use tracing::{debug, error, info, warn, Level, Metadata};
use tracing_subscriber::{filter, fmt::format::FmtSpan, prelude::*};
use vm::{ChannelId, CompletedOperation, OperationId};

#[derive(StructOpt, Debug)]
#[structopt(name = "candy", about = "The 🍭 Candy CLI.")]
enum CandyOptions {
    Build(CandyBuildOptions),
    Run(CandyRunOptions),
    Fuzz(CandyFuzzOptions),
    Lsp,
}

#[derive(StructOpt, Debug)]
struct CandyBuildOptions {
    #[structopt(long)]
    debug: bool,

    #[structopt(long)]
    watch: bool,

    #[structopt(parse(from_os_str))]
    file: PathBuf,
}

#[derive(StructOpt, Debug)]
struct CandyRunOptions {
    #[structopt(long)]
    debug: bool,

    #[structopt(parse(from_os_str))]
    file: PathBuf,
}

#[derive(StructOpt, Debug)]
struct CandyFuzzOptions {
    #[structopt(parse(from_os_str))]
    file: PathBuf,
}

#[tokio::main]
async fn main() {
    init_logger();
    match CandyOptions::from_args() {
        CandyOptions::Build(options) => build(options),
        CandyOptions::Run(options) => run(options),
        CandyOptions::Fuzz(options) => fuzz(options).await,
        CandyOptions::Lsp => lsp().await,
    }
}

fn build(options: CandyBuildOptions) {
    let module = Module::from_package_root_and_file(
        current_dir().unwrap(),
        options.file.clone(),
        ModuleKind::Code,
    );
    raw_build(module.clone(), options.debug);

    if options.watch {
        let (tx, rx) = channel();
        let mut watcher = watcher(tx, Duration::from_secs(1)).unwrap();
        watcher
            .watch(&options.file, RecursiveMode::Recursive)
            .unwrap();
        loop {
            match rx.recv() {
                Ok(_) => {
                    raw_build(module.clone(), options.debug);
                }
                Err(e) => error!("watch error: {e:#?}"),
            }
        }
    }
}
fn raw_build(module: Module, debug: bool) -> Option<Arc<Lir>> {
    let db = Database::default();

    tracing::span!(Level::DEBUG, "Parsing string to RCST").in_scope(|| {
        let rcst = db
            .rcst(module.clone())
            .unwrap_or_else(|err| panic!("Error parsing file `{}`: {:?}", module, err));
        if debug {
            module.dump_associated_debug_file("rcst", &format!("{:#?}\n", rcst));
        }
    });

    tracing::span!(Level::DEBUG, "Turning RCST to CST").in_scope(|| {
        let cst = db.cst(module.clone()).unwrap();
        if debug {
            module.dump_associated_debug_file("cst", &format!("{:#?}\n", cst));
        }
    });

    tracing::span!(Level::DEBUG, "Abstracting CST to AST").in_scope(|| {
        let (asts, ast_cst_id_map) = db.ast(module.clone()).unwrap();
        if debug {
            module.dump_associated_debug_file(
                "ast",
                &format!("{}\n", asts.iter().map(|ast| format!("{}", ast)).join("\n")),
            );
            module.dump_associated_debug_file(
                "ast_to_cst_ids",
                &ast_cst_id_map
                    .keys()
                    .into_iter()
                    .sorted_by_key(|it| it.local)
                    .map(|key| format!("{key} -> {}\n", ast_cst_id_map[key].0))
                    .join(""),
            );
        }
    });

    tracing::span!(Level::DEBUG, "Turning AST to HIR").in_scope(|| {
        let (hir, hir_ast_id_map) = db.hir(module.clone()).unwrap();
        if debug {
            module.dump_associated_debug_file("hir", &format!("{}", hir));
            module.dump_associated_debug_file(
                "hir_to_ast_ids",
                &hir_ast_id_map
                    .keys()
                    .into_iter()
                    .map(|key| format!("{key} -> {}\n", hir_ast_id_map[key]))
                    .join(""),
            );
        }
        let mut errors = vec![];
        hir.collect_errors(&mut errors);
        for CompilerError { span, payload, .. } in errors {
            let (start_line, start_col) = db.offset_to_lsp(module.clone(), span.start);
            let (end_line, end_col) = db.offset_to_lsp(module.clone(), span.end);
            warn!("{start_line}:{start_col} – {end_line}:{end_col}: {payload:?}");
        }
    });

    let lir = tracing::span!(Level::DEBUG, "Lowering HIR to LIR").in_scope(|| {
        let lir = db.lir(module.clone()).unwrap();
        if debug {
            module.dump_associated_debug_file("lir", &format!("{lir}"));
        }
        lir
    });

    Some(lir)
}

fn run(options: CandyRunOptions) {
    let module = Module::from_package_root_and_file(
        current_dir().unwrap(),
        options.file.clone(),
        ModuleKind::Code,
    );
    let db = Database::default();

    if raw_build(module.clone(), false).is_none() {
        warn!("Build failed.");
        return;
    };
    // TODO: Optimize the code before running.

    let path_string = options.file.to_string_lossy();
    debug!("Running `{path_string}`.");

    let module_closure = Closure::of_module(&db, module.clone()).unwrap();
    let mut tracer = FullTracer::new();

    let mut vm = Vm::new();
    vm.set_up_for_running_module_closure(module_closure);
    vm.run(&mut DbUseProvider { db: &db }, &mut RunForever, &mut tracer);
    if let Status::WaitingForOperations = vm.status() {
        error!("The module waits on channel operations. Perhaps, the code tried to read from a channel without sending a packet into it.");
        // TODO: Show stack traces of all fibers?
    }
    let result = vm.tear_down();

    if options.debug {
        module.dump_associated_debug_file("trace", &format!("{tracer:?}"));
    }

    let (mut heap, exported_definitions): (_, Struct) = match result {
        ExecutionResult::Finished(return_value) => {
            debug!("The module exports these definitions: {return_value:?}",);
            let exported = return_value
                .heap
                .get(return_value.address)
                .data
                .clone()
                .try_into()
                .unwrap();
            (return_value.heap, exported)
        }
        ExecutionResult::Panicked {
            reason,
            responsible,
        } => {
            error!("The module panicked because {reason}.");
            if let Some(responsible) = responsible {
                error!("{responsible} is responsible.");
            } else {
                error!("Some top-level code panics.");
            }
            error!(
                "This is the stack trace:\n{}",
                tracer.format_panic_stack_trace_to_root_fiber(&db)
            );
            return;
        }
    };

    let main = heap.create_symbol("Main".to_string());
    let main = match exported_definitions.get(&heap, main) {
        Some(main) => main,
        None => {
            error!("The module doesn't contain a main function.");
            return;
        }
    };

    debug!("Running main function.");
    // TODO: Add more environment stuff.
    let mut vm = Vm::new();
    let mut stdout = StdoutService::new(&mut vm);
    let environment = {
        let stdout_symbol = heap.create_symbol("Stdout".to_string());
        let stdout_port = heap.create_send_port(stdout.channel);
        heap.create_struct(HashMap::from([(stdout_symbol, stdout_port)]))
    };
    tracer.in_fiber_tracer(FiberId::root()).call_started(
        &heap,
        Id::new(module, vec!["main".to_string()]),
        main,
        vec![environment],
    );
    vm.set_up_for_running_closure(heap, main, &[environment]);
    loop {
        match vm.status() {
            Status::CanRun => {
                debug!("VM still running.");
                vm.run(&mut DbUseProvider { db: &db }, &mut RunForever, &mut tracer);
            }
            Status::WaitingForOperations => {
                todo!("VM can't proceed until some operations complete.");
            }
            _ => break,
        }
        stdout.run(&mut vm);
        for channel in vm.unreferenced_channels.iter().copied().collect_vec() {
            if channel != stdout.channel {
                vm.free_channel(channel);
            }
        }
    }
    match vm.tear_down() {
        ExecutionResult::Finished(return_value) => {
            tracer
                .in_fiber_tracer(FiberId::root())
                .call_ended(&return_value.heap, return_value.address);
            debug!("The main function returned: {return_value:?}");
        }
        ExecutionResult::Panicked {
            reason,
            responsible,
        } => {
            error!("The main function panicked because {reason}.");
            if let Some(responsible) = responsible {
                error!("{responsible} is responsible.");
            } else {
                error!("A needs directly in the main function panicks. Perhaps the main functions expects more in the environment.");
            }
            error!(
                "This is the stack trace:\n{}",
                tracer.format_panic_stack_trace_to_root_fiber(&db)
            );
        }
    }
}

/// A state machine that corresponds to a loop that always calls `receive` on
/// the stdout channel and then logs that packet.
struct StdoutService {
    channel: ChannelId,
    current_receive: OperationId,
}
impl StdoutService {
    fn new(vm: &mut Vm) -> Self {
        let channel = vm.create_channel(1);
        let current_receive = vm.receive(channel);
        Self {
            channel,
            current_receive,
        }
    }
    fn run(&mut self, vm: &mut Vm) {
        if let Some(CompletedOperation::Received { packet }) =
            vm.completed_operations.remove(&self.current_receive)
        {
            info!("Sent to stdout: {packet:?}");
            self.current_receive = vm.receive(self.channel);
        }
    }
}

async fn fuzz(options: CandyFuzzOptions) {
    let module = Module::from_package_root_and_file(
        current_dir().unwrap(),
        options.file.clone(),
        ModuleKind::Code,
    );

    if raw_build(module.clone(), false).is_none() {
        warn!("Build failed.");
        return;
    }

    debug!("Fuzzing `{module}`.");
    let db = Database::default();
    fuzzer::fuzz(&db, module).await;
}

async fn lsp() {
    info!("Starting language server…");
    let (service, socket) = LspService::new(CandyLanguageServer::from_client);
    Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
        .serve(service)
        .await;
}

fn init_logger() {
    let console_log = tracing_subscriber::fmt::layer()
        .compact()
        .with_span_events(FmtSpan::ENTER)
        .with_filter(filter::filter_fn(|metadata| {
            // For external packages, show only the error logs.
            metadata.level() <= &Level::ERROR
                || metadata
                    .module_path()
                    .unwrap_or_default()
                    .starts_with("candy")
        }))
        .with_filter(filter::filter_fn(level_for("candy::compiler", Level::WARN)))
        .with_filter(filter::filter_fn(level_for(
            "candy::language_server",
            Level::DEBUG,
        )))
        .with_filter(filter::filter_fn(level_for("candy::vm", Level::DEBUG)))
        .with_filter(filter::filter_fn(level_for(
            "candy::vm::heap",
            Level::DEBUG,
        )));
    tracing_subscriber::registry().with(console_log).init();
}
fn level_for(module: &'static str, level: Level) -> impl Fn(&Metadata) -> bool {
    move |metadata| {
        if metadata
            .module_path()
            .unwrap_or_default()
            .starts_with(module)
        {
            metadata.level() <= &level
        } else {
            true
        }
    }
}
