mod cli;
mod kv_page_handoff;
mod report;
mod router_validation;
mod runner;
mod support;

use anyhow::Result;
use clap::Parser;

use crate::{
    cli::{Cli, CommandKind},
    kv_page_handoff::kv_page_handoff,
    router_validation::router_validation,
    runner::{chain, dtype_matrix, single_step, split_scan, state_handoff},
};

fn main() -> Result<()> {
    match Cli::parse().command {
        CommandKind::SingleStep(args) => single_step(args),
        CommandKind::Chain(args) => chain(args),
        CommandKind::SplitScan(args) => split_scan(args),
        CommandKind::DtypeMatrix(args) => dtype_matrix(args),
        CommandKind::StateHandoff(args) => state_handoff(args),
        CommandKind::RouterValidation(args) => router_validation(args),
        CommandKind::KvPageHandoff(args) => kv_page_handoff(args),
    }
}
