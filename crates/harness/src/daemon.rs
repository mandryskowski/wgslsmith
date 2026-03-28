use crate::{ExecutionInput, ExecutionOutput, WebGPUState};
use clap::Parser;
use std::io::{BufReader, BufWriter, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::ops::Div;
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
    pub fn new(dawn_flags: Vec<String>) -> Self {
        DaemonServer {
            webgpu_state: crate::WebGPUState::new(dawn_flags),
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
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let safe_addr = addr.replace([':', '.'], "_");
    let log_prefix = format!("wgslsmith_daemon_{}_{}", timestamp, safe_addr);

    let log_file_path = std::env::temp_dir().join(format!("{}.log", log_prefix));
    let log_file_err_path = std::env::temp_dir().join(format!("{}_err.log", log_prefix));

    #[cfg(not(target_os = "windows"))]
    {
        use std::process::Stdio;

        let log_file = std::fs::File::create(&log_file_path)?;
        let log_file_err = std::fs::File::create(&log_file_err_path)?;

        let mut command = std::process::Command::new(std::env::current_exe()?);
        command
            .arg("harness")
            .arg("daemon")
            .args(["--address", addr])
            .stdin(Stdio::null())
            .stdout(Stdio::from(log_file))
            .stderr(Stdio::from(log_file_err));

        let child = command.spawn()?;
        eprintln!("spawned {}", child.id());
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};

        let exe_path = std::env::current_exe()?;

        let inner_cmd = format!(
            "\"{}\" harness daemon --address {} > \"{}\" 2> \"{}\"",
            exe_path.display(),
            addr,
            log_file_path.display(),
            log_file_err_path.display()
        );

        let cmd_line = format!("cmd.exe /c \"{}\"", inner_cmd);

        let ps_command = format!(
            "$startup = New-CimInstance -ClassName Win32_ProcessStartup -ClientOnly; \
             $startup.ShowWindow = 0; \
             Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{{CommandLine='{}'; ProcessStartupInformation=$startup}}",
            cmd_line.replace('\'', "''") // Escape single quotes for PowerShell
        );

        let mut command = Command::new("powershell");
        command
            .args([
                "-NoProfile",
                "-WindowStyle",
                "Hidden",
                "-Command",
                &ps_command,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);

        let mut child = command.spawn()?;
        child.wait()?;

        eprintln!("spawned daemon invisibly via WMI at {}", addr);
    }

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
