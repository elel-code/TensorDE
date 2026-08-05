use std::{
    alloc::{GlobalAlloc, Layout, System},
    error::Error,
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use compio::{
    driver::{DriverType, ProactorBuilder},
    runtime::RuntimeBuilder,
};
use tensor_dbus::{
    BusAddress, Connection, MethodCall, PendingReply, RequestNameFlags, RequestNameReply,
    reply_method,
};

const DESTINATION: &str = "org.tensor.DBusBenchmark";
const PATH: &str = "/org/tensor/DBusBenchmark";
const INTERFACE: &str = "org.tensor.DBusBenchmark";

struct CountingAllocator;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);

// SAFETY: every operation delegates to the system allocator with the original
// pointer and layout. The counters do not participate in allocation itself.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        // SAFETY: the caller supplies the layout required by GlobalAlloc.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        // SAFETY: the caller supplies the layout required by GlobalAlloc.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: the caller returns a pointer allocated with this layout.
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        // SAFETY: the caller returns a compatible allocation and new size.
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

#[derive(Clone, Copy)]
struct Measurement {
    elapsed: Duration,
    allocations: u64,
    allocated_bytes: u64,
}

struct PrivateBus(Child);

impl PrivateBus {
    fn start() -> Result<(Self, String), Box<dyn Error>> {
        let mut child = Command::new("dbus-daemon")
            .args([
                "--session",
                "--nofork",
                "--nopidfile",
                "--print-address=1",
                "--address=unix:tmpdir=/tmp",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap()).read_line(&mut address)?;
        if address.is_empty() {
            return Err("dbus-daemon did not announce an address".into());
        }
        Ok((Self(child), address.trim().to_owned()))
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn argument(index: usize, default: usize, name: &str) -> Result<usize, Box<dyn Error>> {
    let Some(value) = std::env::args().nth(index) else {
        return Ok(default);
    };
    let value = value.parse()?;
    if value == 0 {
        return Err(format!("{name} must be greater than zero").into());
    }
    Ok(value)
}

async fn round_trips(
    connection: &mut Connection,
    operations: usize,
    batch_size: usize,
    payload: &[u8],
) -> tensor_dbus::Result<Measurement> {
    let allocations = ALLOCATIONS.load(Ordering::Relaxed);
    let allocated_bytes = ALLOCATED_BYTES.load(Ordering::Relaxed);
    let started = Instant::now();
    let mut completed = 0;
    while completed < operations {
        let current_batch = batch_size.min(operations - completed);
        let mut pending: Vec<PendingReply<Vec<u8>>> = Vec::with_capacity(current_batch);
        for _ in 0..current_batch {
            pending.push(
                connection
                    .send_call(Some(DESTINATION), PATH, Some(INTERFACE), "Echo", &payload)
                    .await?,
            );
        }
        for reply in pending {
            let returned = reply.wait(connection).await?;
            assert_eq!(returned, payload);
        }
        completed += current_batch;
    }
    Ok(Measurement {
        elapsed: started.elapsed(),
        allocations: ALLOCATIONS.load(Ordering::Relaxed) - allocations,
        allocated_bytes: ALLOCATED_BYTES.load(Ordering::Relaxed) - allocated_bytes,
    })
}

fn print_result(label: &str, operations: usize, measurement: Measurement) {
    let seconds = measurement.elapsed.as_secs_f64();
    println!(
        "{label}: operations={operations} elapsed_ms={:.3} operations_per_second={:.0} us_per_operation={:.3} allocations={} allocations_per_operation={:.3} allocated_bytes={} allocated_bytes_per_operation={:.1}",
        seconds * 1_000.0,
        operations as f64 / seconds,
        seconds * 1_000_000.0 / operations as f64,
        measurement.allocations,
        measurement.allocations as f64 / operations as f64,
        measurement.allocated_bytes,
        measurement.allocated_bytes as f64 / operations as f64,
    );
}

fn main() -> Result<(), Box<dyn Error>> {
    let throughput_operations = argument(1, 10_000, "throughput operations")?;
    let payload_bytes = argument(2, 32, "payload bytes")?;
    let batch_size = argument(3, 64, "batch size")?;
    let latency_operations = argument(4, 1_000, "latency operations")?;
    let warmup_operations = latency_operations.min(256);
    let total_operations = warmup_operations
        .checked_add(latency_operations)
        .and_then(|total| total.checked_add(throughput_operations))
        .ok_or("operation count overflow")?;
    let (_bus, announced_address) = PrivateBus::start()?;
    let address = BusAddress::parse(&announced_address)?;

    let mut proactor = ProactorBuilder::new();
    proactor.driver_type(DriverType::IoUring);
    let mut builder = RuntimeBuilder::new();
    builder.with_proactor(proactor);
    let runtime = builder.build()?;
    runtime.block_on(async move {
        let mut server = Connection::connect_bus(address.clone()).await?;
        assert_eq!(
            server
                .request_name(DESTINATION, RequestNameFlags::default())
                .await?,
            RequestNameReply::PrimaryOwner
        );
        let server_task = compio::runtime::spawn(async move {
            let mut served = 0;
            while served < total_operations {
                let message = server.receive().await?;
                let Some(call) = MethodCall::new(message) else {
                    continue;
                };
                if call.member() != "Echo" {
                    continue;
                }
                let body: Vec<u8> = call.body()?;
                reply_method(&mut server, &call, &body).await?;
                served += 1;
            }
            Ok::<_, Box<dyn Error>>(())
        });

        let mut client = Connection::connect_bus(address).await?;
        let payload = vec![0xa5; payload_bytes];
        round_trips(&mut client, warmup_operations, 1, &payload).await?;
        let latency = round_trips(&mut client, latency_operations, 1, &payload).await?;
        let throughput =
            round_trips(&mut client, throughput_operations, batch_size, &payload).await?;
        server_task.await??;

        println!("payload_bytes={payload_bytes} batch_size={batch_size}");
        print_result("sequential", latency_operations, latency);
        print_result("batched", throughput_operations, throughput);
        Ok::<_, Box<dyn Error>>(())
    })?;
    Ok(())
}
