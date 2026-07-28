use desktop_shell::ShellRuntime;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    ShellRuntime::connect()?.run()?;
    Ok(())
}
