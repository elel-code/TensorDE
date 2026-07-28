use fika_core::{HelperBus, run_dbus_service};
use std::env;

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

    if let Err(err) = async_io::block_on(run_dbus_service(bus)) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn print_help() {
    println!(
        "Usage: fika-privileged-helper [--system-bus | --session-bus ADDRESS]\n\n\
         --system-bus starts the installable system D-Bus service and checks\n\
         polkit per method. --session-bus is a development fallback intended\n\
         for pkexec and refuses to run without PKEXEC_UID."
    );
}
