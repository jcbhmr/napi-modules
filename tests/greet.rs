use std::{env, error::Error, process::Command};

#[test]
fn test_greet() -> Result<(), Box<dyn Error>> {
    let status = Command::new(env::var("CARGO")?)
        .args(&["build", "--package=examples-greet"])
        .status()?;
    if !status.success() {
        return Err(format!("build examples-greet failed: {:?}", status).into());
    }
    _ = fs_err::remove_file("target/debug/examples_greet.node");
    fs_err::rename(
        "target/debug/libexamples_greet.so",
        "target/debug/examples_greet.node",
    )?;
    let status = Command::new("node")
        .args(&["target/debug/examples_greet.node", "Alan Turing"])
        .status()?;
    if !status.success() {
        return Err(format!("run node failed: {:?}", status).into());
    }
    Ok(())
}