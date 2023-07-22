//! rnote-cli
//!
//! The cli interface is not (yet) stable and could change at any time.

pub(crate) mod cli;
pub(crate) mod export;

fn main() -> anyhow::Result<()> {
    smol::block_on(async { cli::run().await })
}
