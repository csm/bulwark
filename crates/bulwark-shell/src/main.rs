fn main() -> anyhow::Result<()> {
    let mut host = bulwark_runtime::RuntimeHost::new(cljrs_runtime::ExecutionMode::Tiered)?;
    println!("{}", host.eval("(bulwark.core/bootstrap-status)")?);
    Ok(())
}
