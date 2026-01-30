use crate::dawn::DawnState;
use crate::wgpu::WgpuState;
use crate::{ExecutionInput, ExecutionOutput, WebGPUState};
use clap::Parser;
use std::io::{BufReader, BufWriter, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::ops::Div;
use std::process::Stdio;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use types::ConfigId;

#[derive(Parser)]
pub struct DaemonOptions {
    /// Daemon bind address.
    #[clap(short, long, action, default_value = "127.0.0.1:9000")]
    address: String,

    /// Timeout since the last received request in seconds.
    #[clap(long, action, default_value = "300")]
    pub inactivity_timeout: u64,
}

#[derive(bincode::Decode, bincode::Encode)]
pub enum DaemonRequest {
    Run(DaemonRunRequest),
}

#[derive(bincode::Decode, bincode::Encode)]
pub struct DaemonRunRequest {
    config: ConfigId,
    execution_input: ExecutionInput,
}

#[derive(bincode::Decode, bincode::Encode)]
pub struct DaemonRunResponse {
    result: Result<ExecutionOutput, String>,
}

pub struct DaemonServer {
    webgpu_state: WebGPUState,
}

impl DaemonServer {
    pub fn new() -> Self {
        DaemonServer {
            webgpu_state: WebGPUState {
                dawn_state: DawnState::new(),
                wgpu_state: WgpuState::new(),
            },
        }
    }

    pub fn main_loop(&mut self, options: DaemonOptions) -> eyre::Result<()> {
        let listener = TcpListener::bind(options.address)?;
        let address = listener.local_addr().unwrap();
        println!("Server listening at {address}");

        let last_activity = Arc::new(Mutex::new(Instant::now()));

        self.spawn_inactivity_watchdog(
            Duration::from_secs(options.inactivity_timeout),
            last_activity.clone(),
        );

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    *last_activity.lock().unwrap() = Instant::now();
                    if let Err(e) = self.process_request(stream) {
                        eprintln!("Error handling client: {}", e);
                    }
                }
                Err(e) => eprintln!("Connection error: {}", e),
            }
        }

        Ok(())
    }

    fn spawn_inactivity_watchdog(
        &self,
        timeout: Duration,
        last_activity_check: Arc<Mutex<Instant>>,
    ) {
        std::thread::spawn(move || loop {
            std::thread::sleep(timeout.div(4));
            let last = *last_activity_check.lock().unwrap();
            if last.elapsed() > timeout {
                println!(
                    "Daemon has been inactive for {}s. Shutting down.",
                    last.elapsed().as_secs_f32()
                );
                std::process::exit(0);
            }
        });
    }

    fn process_request(&mut self, stream: TcpStream) -> eyre::Result<()> {
        let mut reader = BufReader::new(&stream);
        let mut writer = BufWriter::new(&stream);

        let req: DaemonRequest =
            bincode::decode_from_std_read(&mut reader, bincode::config::standard())?;

        match req {
            DaemonRequest::Run(req) => {
                let done_flag = Arc::new(AtomicBool::new(false));
                let done_clone = done_flag.clone();

                let timeout = req
                    .execution_input
                    .timeout
                    .unwrap_or(Duration::from_secs(60));

                std::thread::spawn(move || {
                    std::thread::sleep(timeout);
                    if !done_clone.load(Ordering::Relaxed) {
                        eprintln!("Execution timed out after {}s.", timeout.as_secs());
                        std::process::exit(1);
                    }
                });

                let result = crate::execute_config(
                    &req.execution_input.shader,
                    &req.execution_input.pipeline_desc,
                    &req.config,
                    Some(&mut self.webgpu_state),
                );

                done_flag.store(true, Ordering::Relaxed);

                let response_result = match result {
                    Ok(buffers) => Ok(ExecutionOutput { buffers }),
                    Err(e) => Err(format!("{:?}", e)),
                };

                let response = DaemonRunResponse {
                    result: response_result,
                };

                bincode::encode_into_std_write(response, &mut writer, bincode::config::standard())?;
                writer.flush()?;
            }
        }
        Ok(())
    }
}

pub fn daemon_exec(config: ConfigId) -> eyre::Result<()> {
    let input: ExecutionInput =
        bincode::decode_from_std_read(&mut std::io::stdin(), bincode::config::standard())?;

    let address = format!("127.0.0.1:{}", 9000 + input.tid);

    let stream = match TcpStream::connect_timeout(
        &SocketAddr::from_str(&address).unwrap(),
        Duration::from_millis(500),
    ) {
        Ok(s) => s,
        Err(_) => {
            eprintln!("Daemon not running. Spawning...");
            spawn_daemon(&address)?;
            wait_for_connection(&address)?
        }
    };

    stream.set_read_timeout(Some(Duration::from_secs(60)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;

    let mut reader = BufReader::new(&stream);
    let mut writer = BufWriter::new(&stream);

    let req = DaemonRequest::Run(DaemonRunRequest {
        config,
        execution_input: input,
    });

    bincode::encode_into_std_write(req, &mut writer, bincode::config::standard())?;

    writer.flush()?;

    let response: DaemonRunResponse =
        match bincode::decode_from_std_read(&mut reader, bincode::config::standard()) {
            Ok(res) => res,
            Err(e) => {
                if let bincode::error::DecodeError::UnexpectedEnd { .. } = e {
                    panic!("The harness daemon crashed or closed the connection unexpectedly.");
                }
                panic!("Unknown error {:?}", e);
            }
        };

    if let Err(e) = &response.result {
        panic!("Daemon execution failed: {}", e);
    }

    bincode::encode_into_std_write(
        response.result.unwrap(),
        &mut std::io::stdout(),
        bincode::config::standard(),
    )?;

    Ok(())
}

fn spawn_daemon(addr: &str) -> std::io::Result<()> {
    let log_file = std::fs::File::create(std::env::temp_dir().join("wgslsmith_daemon.log"))?;
    let log_file_err =
        std::fs::File::create(std::env::temp_dir().join("wgslsmith_daemon_err.log"))?;

    let mut command = std::process::Command::new(std::env::current_exe()?);

    command
        .arg("harness")
        .arg("daemon")
        .args(["--address", addr])
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_file_err));

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::io::AsRawHandle;
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x00000008;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        // Use DETACHED_PROCESS | CREATE_NO_WINDOW to ensure it runs backgrounded
        // and doesn't die when the parent console interacts with it.
        command.creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW);

        unsafe {
            extern "system" {
                fn SetHandleInformation(
                    hObject: std::os::windows::io::RawHandle,
                    dwMask: u32,
                    dwFlags: u32,
                ) -> i32;
            }
            const HANDLE_FLAG_INHERIT: u32 = 0x00000001;

            let stdout_handle = std::io::stdout().as_raw_handle();
            let stderr_handle = std::io::stderr().as_raw_handle();

            SetHandleInformation(stdout_handle, HANDLE_FLAG_INHERIT, 0);
            SetHandleInformation(stderr_handle, HANDLE_FLAG_INHERIT, 0);
        }
    }

    let child = command.spawn()?;

    eprintln!("spawned {}", child.id());
    Ok(())
}

fn wait_for_connection(addr: &str) -> color_eyre::Result<TcpStream> {
    for _ in 0..300 {
        // Try for 30 seconds (100ms * 300)
        if let Ok(s) = TcpStream::connect(addr) {
            return Ok(s);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    eyre::bail!("Could not connect to daemon after spawning");
}
