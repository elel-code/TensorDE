use std::env;
use tensor_files_core::{HelperBus, run_dbus_service};

fn main() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-h" | "--help"))
    {
        print_help();
        return;
    }

    let system_bus = args.iter().any(|arg| arg == "--system-bus");
    let session_bus_address = args
        .windows(2)
        .find(|window| window[0] == "--session-bus")
        .map(|window| window[1].clone());
    let bus = if system_bus {
        HelperBus::System
    } else {
        HelperBus::Session {
            session_bus_address,
        }
    };

    let runtime = match compio::runtime::RuntimeBuilder::new().build() {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("cannot create privileged helper Compio runtime: {err}");
            std::process::exit(1);
        }
    };
    if let Err(err) = runtime.block_on(run_dbus_service(bus)) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn print_help() {
    println!(
        "Usage: tensor-files-privileged-helper [--system-bus | --session-bus ADDRESS]\n\n\
         --system-bus starts the installable system D-Bus service and checks\n\
         polkit per method. --session-bus is a development fallback intended\n\
         for pkexec and refuses to run without PKEXEC_UID."
    );
}
