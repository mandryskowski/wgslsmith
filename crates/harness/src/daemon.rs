use crate::{ExecutionInput, ExecutionOutput, WebGPUState};
use clap::Parser;
use std::io::{BufReader, BufWriter, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::ops::Div;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
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

    #[clap(long, action)]
    pub log_err_path: Option<String>,
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
pub enum DaemonResponse {
    Ack,
    Result(Result<ExecutionOutput, String>),
}

pub struct DaemonServer {
    webgpu_state: WebGPUState,
    active_requests: Arc<AtomicUsize>,
    shutting_down: Arc<AtomicBool>,
}

impl DaemonServer {
    pub fn new(dawn_flags: crate::DawnFlags) -> Self {
        DaemonServer {
            webgpu_state: crate::WebGPUState::new(dawn_flags),
            active_requests: Arc::new(AtomicUsize::new(0)),
            shutting_down: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn main_loop(&mut self, options: DaemonOptions) -> eyre::Result<()> {
        let start_time = Instant::now();
        let mut last_log_time = start_time;
        let log_err_path = options.log_err_path.clone();

        let listener = loop {
            match TcpListener::bind(&options.address) {
                Ok(l) => break l,
                Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                    let now = Instant::now();

                    if now.duration_since(start_time) >= Duration::from_secs(30) {
                        eyre::bail!(
                            "Timeout waiting for port {} to become available after 30s",
                            options.address
                        );
                    }

                    if now.duration_since(last_log_time) >= Duration::from_secs(5) {
                        println!(
                            "Waiting for port {} to become available...",
                            options.address
                        );
                        last_log_time = now;
                    }

                    // Wait for the old daemon to free the port
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) => return Err(e.into()),
            }
        };

        let address = listener.local_addr().unwrap();
        println!("Server listening at {address}");

        listener.set_nonblocking(true)?;
        let listener_arc = Arc::new(Mutex::new(Some(listener)));

        let last_activity = Arc::new(Mutex::new(Instant::now()));

        self.spawn_inactivity_watchdog(
            Duration::from_secs(options.inactivity_timeout),
            last_activity.clone(),
        );

        loop {
            if self.shutting_down.load(Ordering::SeqCst) {
                break;
            }

            let accept_res = {
                let lock = listener_arc.lock().unwrap();
                if let Some(l) = lock.as_ref() {
                    l.accept()
                } else {
                    break;
                }
            };

            match accept_res {
                Ok((stream, _)) => {
                    *last_activity.lock().unwrap() = Instant::now();

                    if let Err(e) = stream.set_nonblocking(false) {
                        eprintln!("Warning: failed to set stream to blocking: {}", e);
                    }

                    if let Err(e) =
                        self.process_request(stream, listener_arc.clone(), log_err_path.as_deref())
                    {
                        eprintln!("Error handling client: {}", e);
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => eprintln!("Connection error: {}", e),
            }
        }

        while self.active_requests.load(Ordering::SeqCst) > 0 {
            std::thread::sleep(Duration::from_millis(100));
        }

        if self.shutting_down.load(Ordering::SeqCst) {
            std::process::exit(1);
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

    fn process_request(
        &mut self,
        stream: TcpStream,
        listener_arc: Arc<Mutex<Option<TcpListener>>>,
        log_err_path: Option<&str>,
    ) -> eyre::Result<()> {
        let mut reader = BufReader::new(&stream);
        let mut writer = BufWriter::new(&stream);

        let req: DaemonRequest =
            bincode::decode_from_std_read(&mut reader, bincode::config::standard())?;

        match req {
            DaemonRequest::Run(req) => {
                // Send an ACK to tell the client we have successfully started processing their request
                bincode::encode_into_std_write(
                    DaemonResponse::Ack,
                    &mut writer,
                    bincode::config::standard(),
                )?;
                writer.flush()?;

                // Print out the seed
                println!(
                    "{}",
                    req.execution_input.shader.lines().next().unwrap_or("")
                );

                let state = Arc::new(AtomicU8::new(0)); // 0: running, 1: finished, 2: timed out
                let state_clone = state.clone();

                let timeout = req
                    .execution_input
                    .timeout
                    .unwrap_or(Duration::from_secs(60))
                    + Duration::from_secs(1);

                let shutting_down = self.shutting_down.clone();
                let listener_clone = listener_arc.clone();
                let active_requests = self.active_requests.clone();

                active_requests.fetch_add(1, Ordering::SeqCst);

                std::thread::spawn(move || {
                    std::thread::sleep(timeout);

                    if state_clone
                        .compare_exchange(0, 2, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                    {
                        eprintln!(
                            "Execution timed out after {}s. Freeing port.",
                            timeout.as_secs()
                        );
                        shutting_down.store(true, Ordering::SeqCst);

                        if let Some(l) = listener_clone.lock().unwrap().take() {
                            drop(l);
                        }

                        if active_requests.fetch_sub(1, Ordering::SeqCst) == 1 {
                            std::process::exit(1);
                        }
                    }
                });

                let size_before = log_err_path
                    .and_then(|p| std::fs::metadata(p).ok())
                    .map(|m| m.len())
                    .unwrap_or(0);

                let result = crate::execute_config(
                    &req.execution_input.shader,
                    &req.execution_input.pipeline_desc,
                    &req.config,
                    req.execution_input.compile_only,
                    Some(&mut self.webgpu_state),
                );

                let stderr = log_err_path
                    .and_then(|p| std::fs::File::open(p).ok())
                    .and_then(|mut f| {
                        use std::io::{Read, Seek};
                        if f.seek(std::io::SeekFrom::Start(size_before)).is_ok() {
                            let mut s = String::new();
                            let _ = f.read_to_string(&mut s);
                            Some(s)
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();

                if state
                    .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    let remaining = self.active_requests.fetch_sub(1, Ordering::SeqCst) - 1;

                    let response_result = match result {
                        Ok(buffers) => Ok(ExecutionOutput { buffers, stderr }),
                        Err(e) => Err(format!("{:?}", e)),
                    };

                    // The socket might be broken if the client timed out on their end.
                    // It is safe to ignore errors here.
                    let _ = bincode::encode_into_std_write(
                        DaemonResponse::Result(response_result),
                        &mut writer,
                        bincode::config::standard(),
                    );
                    let _ = writer.flush();

                    if self.shutting_down.load(Ordering::SeqCst) && remaining == 0 {
                        std::process::exit(1);
                    }
                }
            }
        }
        Ok(())
    }
}

pub fn daemon_exec(config: ConfigId, daemon_port: Option<u16>) -> eyre::Result<()> {
    let input: ExecutionInput =
        bincode::decode_from_std_read(&mut std::io::stdin(), bincode::config::standard())?;

    let base_port = daemon_port.unwrap_or(9000);
    let address = format!("127.0.0.1:{}", base_port + input.tid as u16);

    let req = DaemonRequest::Run(DaemonRunRequest {
        config,
        execution_input: input,
    });

    let mut req_bytes = Vec::new();
    bincode::encode_into_std_write(&req, &mut req_bytes, bincode::config::standard())?;

    let mut attempts = 0;

    let response: Result<ExecutionOutput, String> = loop {
        attempts += 1;

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

        stream.set_read_timeout(Some(Duration::from_secs(65)))?;
        stream.set_write_timeout(Some(Duration::from_secs(10)))?;

        let mut writer = BufWriter::new(stream.try_clone().unwrap());

        if let Err(e) = writer.write_all(&req_bytes) {
            if attempts < 3 {
                eprintln!("Failed to write to daemon: {}. Retrying...", e);
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }
            panic!("Failed to write to daemon: {}", e);
        }

        if let Err(e) = writer.flush() {
            if attempts < 3 {
                eprintln!("Failed to flush to daemon: {}. Retrying...", e);
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }
            panic!("Failed to flush to daemon: {}", e);
        }

        let mut reader = BufReader::new(&stream);

        // Wait for ACK
        match bincode::decode_from_std_read(&mut reader, bincode::config::standard()) {
            Ok(DaemonResponse::Ack) => {
                // If it crashes now, it is genuinely the shader's fault.
                match bincode::decode_from_std_read(&mut reader, bincode::config::standard()) {
                    Ok(DaemonResponse::Result(res)) => break res,
                    Ok(_) => panic!("Unexpected response from daemon instead of Result"),
                    Err(e) => {
                        panic!(
                            "Daemon crashed or closed connection during execution: {:?}",
                            e
                        );
                    }
                }
            }
            Ok(_) => panic!("Unexpected response from daemon instead of ACK"),
            Err(e) => {
                if attempts < 3 {
                    eprintln!("Failed to read ACK from daemon (likely dead from previous run). Retrying...");
                    std::thread::sleep(Duration::from_millis(200));
                    continue;
                }
                panic!(
                    "Failed to connect and start execution after 3 attempts: {:?}",
                    e
                );
            }
        }
    };

    if let Err(e) = &response {
        panic!("Daemon execution failed: {}", e);
    }

    let out = response.unwrap();
    if !out.stderr.is_empty() {
        eprint!("{}", out.stderr);
    }

    bincode::encode_into_std_write(out, &mut std::io::stdout(), bincode::config::standard())?;

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
            .args(["--log-err-path", log_file_err_path.to_str().unwrap()])
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
            "\"{}\" harness daemon --address {} --log-err-path \"{}\" > \"{}\" 2> \"{}\"",
            exe_path.display(),
            addr,
            log_file_err_path.display(),
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
