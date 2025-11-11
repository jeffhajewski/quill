//! Compatibility checking command

use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
pub struct CompatArgs {
    /// Reference to compare against (git ref or registry URL)
    #[arg(short, long)]
    pub against: String,

    /// Proto files to check
    pub protos: Vec<String>,

    /// Fail on breaking changes
    #[arg(long)]
    pub strict: bool,
}

pub fn run(args: CompatArgs) -> Result<()> {
    println!("Checking compatibility...");
    println!("  Against: {}", args.against);
    println!("  Protos: {:?}", args.protos);

    // TODO: Implement compatibility checking
    // This would:
    // 1. Load proto files from current version
    // 2. Load proto files from reference version
    // 3. Compare for breaking changes:
    //    - Removed fields
    //    - Changed field types
    //    - Removed services/methods
    //    - Changed method signatures
    // 4. Report findings
    // 5. Exit with code 2 if breaking changes found (in strict mode)

    anyhow::bail!("Compatibility checking not yet implemented");
}
