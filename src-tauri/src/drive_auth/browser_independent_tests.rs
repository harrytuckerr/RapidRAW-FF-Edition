//! Independent fixed browser-boundary and owned-process tests. No default handler
//! is invoked, no Code callback is sent, and no session-private state is read.
use super::*;
use std::sync::Barrier;

const ZERO_STATE: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const RFC_CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
const FILE_ENCODED: &str = "https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fdrive.file";
const APP_ENCODED: &str = "https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fdrive.appdata";
fn fixed_url(client: &str, port: &str, scope: &str) -> String {
    // Independently specified canonical form bytes, not product validation or
    // the product request serializer used as an expected-value generator.
    format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={client}&redirect_uri=http%3A%2F%2F127.0.0.1%3A{port}%2Foauth2%2Fcallback%2Fgoogle&response_type=code&scope={scope}&state={ZERO_STATE}&code_challenge={RFC_CHALLENGE}&code_challenge_method=S256"
    )
}
#[test]
fn independent_canonical_request_bounds_and_adversarial_aliases() {
    let client = "independent-native.apps.googleusercontent.com";
    for scope in [
        FILE_ENCODED.to_owned(),
        APP_ENCODED.to_owned(),
        format!("{FILE_ENCODED}+{APP_ENCODED}"),
        format!("{APP_ENCODED}+{FILE_ENCODED}"),
    ] {
        for port in ["1", "65535"] {
            assert_eq!(
                validate_destination(&fixed_url(client, port, &scope)),
                Ok(())
            );
        }
    }
    let suffix = ".apps.googleusercontent.com";
    assert_eq!(
        validate_destination(&fixed_url(
            &format!("{}{suffix}", "x".repeat(256 - suffix.len())),
            "1",
            FILE_ENCODED
        )),
        Ok(())
    );
    assert_eq!(
        validate_destination(&fixed_url(
            &format!("{}{suffix}", "x".repeat(257 - suffix.len())),
            "1",
            FILE_ENCODED
        )),
        Err(LaunchError::InvalidDestination)
    );
    for port in ["0", "01", "65536", "-1", "+1"] {
        assert_eq!(
            validate_destination(&fixed_url(client, port, FILE_ENCODED)),
            Err(LaunchError::InvalidDestination)
        );
    }
    let valid = fixed_url(client, "43119", FILE_ENCODED);
    let both = fixed_url(client, "43119", &format!("{FILE_ENCODED}+{APP_ENCODED}"));
    let cases = [
        valid.replacen("https://", "HTTPS://", 1),
        valid.replacen("https://", "http://", 1),
        valid.replacen("accounts.google.com/", "accounts.google.com:443/", 1),
        valid.replacen("accounts.google.com/", "u@accounts.google.com/", 1),
        valid.replacen(
            "accounts.google.com/",
            "accounts.google.com.evil.invalid/",
            1,
        ),
        valid.replacen("accounts.google.com/", "accounts.google.com./", 1),
        valid.replacen("/o/oauth2/v2/auth?", "/o/./oauth2/v2/auth?", 1),
        valid.replacen("/auth?", "/%61uth?", 1),
        format!("{valid}#"),
        format!("{valid}\r\n"),
        valid.replacen("client_id=", "client%5Fid=", 1),
        format!("{valid}&st%61te={ZERO_STATE}"),
        format!("{valid}&state={ZERO_STATE}"),
        format!("{valid}&provider_extension=synthetic"),
        valid.replacen("&response_type=code", "", 1),
        valid.replacen(
            "&response_type=code&scope=",
            "&scope=code&response_type=",
            1,
        ),
        valid.replace(ZERO_STATE, "%"),
        valid.replace(ZERO_STATE, "%G1"),
        valid.replace(ZERO_STATE, "%E2%28%A1"),
        valid.replace(ZERO_STATE, "%FF"),
        valid.replace(ZERO_STATE, &format!("{}B", "A".repeat(42))),
        valid.replace(ZERO_STATE, &format!("{ZERO_STATE}=")),
        valid.replacen("%3A", "%3a", 1),
        valid.replacen("client_id=i", "client_id=%69", 1),
        both.replace('+', "%20"),
        valid.replace("response_type=code", "response_type=token"),
        valid.replace("code_challenge_method=S256", "code_challenge_method=plain"),
        valid.replace(
            FILE_ENCODED,
            "https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fdrive",
        ),
        fixed_url(suffix, "1", FILE_ENCODED),
        fixed_url(client, "1", &format!("{FILE_ENCODED}+{FILE_ENCODED}")),
        format!("{valid}{}", "x".repeat(8193)),
    ];
    for value in cases {
        assert_eq!(
            validate_destination(&value),
            Err(LaunchError::InvalidDestination)
        );
    }
    assert_eq!(strict_form_decode("a%2Bb+c").as_deref(), Ok("a+b c"));
    assert_eq!(strict_form_decode("%F0%9F%93%B7").as_deref(), Ok("📷"));
    assert_eq!(
        strict_form_decode("%ED%A0%80"),
        Err(LaunchError::InvalidDestination)
    );
}
#[test]
fn independent_selected_default_shape_is_built_but_never_invoked() {
    let value = fixed_url(
        "independent-native.apps.googleusercontent.com",
        "7",
        FILE_ENCODED,
    );
    let command = first_default_command(&value).unwrap();
    #[cfg(target_os = "macos")]
    {
        assert!(command.get_program() == "/usr/bin/open");
        assert!(
            command
                .get_args()
                .eq([std::ffi::OsStr::new("--"), std::ffi::OsStr::new(&value)])
        );
        assert!(command.get_envs().next().is_none());
    }
    #[cfg(not(target_os = "macos"))]
    {
        assert!(!command.get_program().is_empty());
    }
    // Command drops here; never spawn/status/output the OS-selected handler.
    let control = TestControl::new();
    let plan = LaunchPlan::Synthetic {
        command: Box::new(Command::new("not-executed-independent-plan")),
        control: Arc::clone(&control),
    };
    let built = plan.command(&value).unwrap();
    assert!(built.get_program() == "not-executed-independent-plan");
    assert!(control.original.lock().unwrap().as_deref() == Some(value.as_str()));
    assert!(!control.started.load(Ordering::Acquire));
}
#[test]
fn independent_real_completion_cas_and_drop_have_only_fixed_winners() {
    let completion = Arc::new(AtomicU8::new(PENDING));
    assert_eq!(claim_completion(&completion), Ok(()));
    drop(CancelOnDrop(Arc::clone(&completion)));
    assert_eq!(completion.load(Ordering::Acquire), COMPLETING);
    assert_eq!(claim_completion(&completion), Err(LaunchError::Cancelled));
    let cancelled = Arc::new(AtomicU8::new(PENDING));
    drop(CancelOnDrop(Arc::clone(&cancelled)));
    assert_eq!(claim_completion(&cancelled), Err(LaunchError::Cancelled));
    assert_eq!(cancelled.load(Ordering::Acquire), CANCELLED);
    let value = Arc::new(AtomicU8::new(PENDING));
    let barrier = Barrier::new(3);
    let outcome = std::thread::scope(|scope| {
        let claim = scope.spawn(|| {
            barrier.wait();
            claim_completion(&value)
        });
        let dropped = scope.spawn(|| {
            barrier.wait();
            drop(CancelOnDrop(Arc::clone(&value)));
        });
        barrier.wait();
        dropped.join().unwrap();
        claim.join().unwrap()
    });
    assert!(matches!(
        (outcome, value.load(Ordering::Acquire)),
        (Ok(()), COMPLETING) | (Err(LaunchError::Cancelled), CANCELLED)
    ));
    // This invokes the real CAS/Drop components; not a forced OS-process race.
}

#[cfg(unix)]
mod actual_processes {
    use super::*;
    use crate::drive_auth::session::{
        AuthorizationOutcome, BrowserLaunchTicket, CancelDisposition, NativeStart, Phase,
        SessionController, StartError, TerminalRecord,
    };
    use std::io::{Read, Write};
    use std::net::{Ipv4Addr, SocketAddr, TcpStream};
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::time::Instant;
    const CLIENT: &str = "independent-launch.apps.googleusercontent.com";
    fn request(scopes: Vec<ApprovedScope>) -> NativeStart {
        NativeStart::new(
            CLIENT.into(),
            "independent-launch".into(),
            "303".into(),
            scopes,
        )
        .unwrap()
    }
    fn both() -> NativeStart {
        request(vec![ApprovedScope::AppData, ApprovedScope::File])
    }
    async fn new_ticket(scopes: Vec<ApprovedScope>) -> (SessionController, BrowserLaunchTicket) {
        let owner = SessionController::new();
        let ticket = tokio::time::timeout(Duration::from_secs(3), owner.begin(request(scopes)))
            .await
            .unwrap()
            .unwrap();
        (owner, ticket)
    }
    async fn wait_for(predicate: impl Fn() -> bool) {
        tokio::time::timeout(Duration::from_secs(3), async {
            while !predicate() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("bounded independent observation");
    }
    async fn flag(value: &AtomicBool) {
        wait_for(|| value.load(Ordering::Acquire)).await;
    }
    async fn terminal(owner: &SessionController) -> TerminalRecord {
        tokio::time::timeout(Duration::from_secs(12), async {
            loop {
                if let Some(value) = owner.snapshot().unwrap().last_authorization {
                    return value;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("original bounded handoff terminal")
    }
    async fn shutdown(owner: &SessionController) {
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(3), owner.shutdown())
                .await
                .unwrap(),
            Ok(())
        );
    }
    fn synthetic(command: Command, control: &Arc<TestControl>) -> LaunchPlan {
        LaunchPlan::Synthetic {
            command: Box::new(command),
            control: Arc::clone(control),
        }
    }
    // Literal program, private path argv only. URL/state never becomes script
    // source or child argv. All children are finite/release-aware and joined by
    // production LaunchGuard; teardown resets sticky test error and release.
    struct Held {
        dir: PathBuf,
        control: Arc<TestControl>,
    }
    impl Held {
        fn new() -> Self {
            let dir =
                std::env::temp_dir().join(format!("independent-browser-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&dir).unwrap();
            Self {
                dir,
                control: TestControl::new(),
            }
        }
        fn plan(&self) -> LaunchPlan {
            let mut child = Command::new("/usr/bin/python3");
            child.arg("-c").arg("import os,sys,time\nready,release,done=sys.argv[1:]\nwith open(ready,'x') as f:f.write(str(os.getpid()))\nend=time.monotonic()+18\nwhile not os.path.exists(release) and time.monotonic()<end:time.sleep(.01)\nopen(done,'x').close()\nsys.exit(0 if os.path.exists(release) else 31)\n").arg(self.dir.join("ready")).arg(self.dir.join("release")).arg(self.dir.join("done"));
            synthetic(child, &self.control)
        }
        async fn started(&self) {
            flag(&self.control.started).await;
            wait_for(|| self.dir.join("ready").exists()).await;
        }
        fn release(&self) {
            std::fs::write(self.dir.join("release"), b"release").unwrap();
        }
        async fn exited_before_reap(&self) {
            struct Observer(Option<Child>);

            impl Drop for Observer {
                fn drop(&mut self) {
                    if let Some(mut observer) = self.0.take() {
                        // This fallback runs on the blocking observer worker only.
                        // Never signal or wait on the launcher PID. Native kill/wait
                        // scheduling is not a hard elapsed-time guarantee.
                        let _ = observer.kill();
                        let reaped = loop {
                            match observer.wait() {
                                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                                result => break result,
                            }
                        };
                        if let Err(error) = reaped {
                            if std::thread::panicking() {
                                eprintln!(
                                    "observer cleanup not established during unwind: {error}"
                                );
                            } else {
                                panic!("observer cleanup not established: {error}");
                            }
                        }
                    }
                }
            }

            struct StopObserver(Arc<AtomicBool>);

            impl Drop for StopObserver {
                fn drop(&mut self) {
                    self.0.store(true, Ordering::Release);
                }
            }

            wait_for(|| self.dir.join("done").exists()).await;
            let pid = std::fs::read_to_string(self.dir.join("ready")).unwrap();
            let parsed = pid.parse::<u32>().unwrap();
            assert!(parsed > 1);
            // The real child is an exited unreaped zombie while wait injection
            // remains true. /bin/ps observes state only; it never signals a PID.
            let parent = std::process::id();
            let observation_deadline = Instant::now() + Duration::from_secs(3);
            let stop = StopObserver(Arc::new(AtomicBool::new(false)));
            let stopped = stop.0.clone();
            let dir = self.dir.clone();
            let worker = tokio::task::spawn_blocking(move || {
                loop {
                    if stopped.load(Ordering::Acquire) || Instant::now() >= observation_deadline {
                        return false;
                    }
                    let output = dir.join(format!("observer-{}", uuid::Uuid::new_v4()));
                    let stdout = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&output)
                        .expect("owned observer output file");
                    let deadline =
                        observation_deadline.min(Instant::now() + Duration::from_millis(250));
                    let observed = match Command::new("/bin/ps")
                        .args(["-o", "ppid=,stat=", "-p", pid.as_str()])
                        .stdin(Stdio::null())
                        .stdout(Stdio::from(stdout))
                        .stderr(Stdio::null())
                        .spawn()
                    {
                        Ok(child) => {
                            let mut observer = Observer(Some(child));
                            let status = loop {
                                if stopped.load(Ordering::Acquire) || Instant::now() >= deadline {
                                    break None;
                                }
                                match observer.0.as_mut().unwrap().try_wait() {
                                    Ok(Some(status)) => {
                                        // try_wait has reaped this observer, never the
                                        // launcher. Disarm only after a known exit.
                                        observer.0 = None;
                                        break Some(status);
                                    }
                                    Ok(None) => {}
                                    Err(error) => {
                                        panic!("observer status not established: {error}")
                                    }
                                }
                                std::thread::sleep(Duration::from_millis(5));
                            };
                            // Timeout/failure/unwind retains the Child until this
                            // worker's guard has killed and reaped the observer.
                            drop(observer);
                            match status {
                                Some(status) => {
                                    // No pipe can fill. Read only after known exit,
                                    // at most 256 bytes plus one overflow byte.
                                    let file = std::fs::File::open(&output)
                                        .expect("owned observer output readable");
                                    let mut bytes = Vec::with_capacity(257);
                                    file.take(257)
                                        .read_to_end(&mut bytes)
                                        .expect("bounded regular observer output read");
                                    let text = String::from_utf8_lossy(&bytes);
                                    let fields = text.split_whitespace().collect::<Vec<_>>();
                                    !stopped.load(Ordering::Acquire)
                                        && Instant::now() < deadline
                                        && bytes.len() <= 256
                                        && status.success()
                                        && fields.len() == 2
                                        && fields[0].parse::<u32>().ok() == Some(parent)
                                        && fields[1].starts_with('Z')
                                }
                                None => false,
                            }
                        }
                        Err(_) => false,
                    };
                    std::fs::remove_file(output).expect("owned observer output removed");
                    if observed && Instant::now() < observation_deadline {
                        return true;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            });
            let observed = tokio::time::timeout_at(
                tokio::time::Instant::from_std(observation_deadline),
                worker,
            )
            .await
            .expect("bounded independent observation")
            .expect("owned observer worker completed");
            drop(stop);
            observed
                .then_some(())
                .expect("bounded independent observation");
        }
    }
    impl Drop for Held {
        fn drop(&mut self) {
            self.control.dispatch.store(true, Ordering::Release);
            self.control.wait_error.store(false, Ordering::Release);
            let _ = std::fs::write(self.dir.join("release"), b"release");
        }
    }

    #[tokio::test]
    async fn independent_generated_bytes_real_null_child_failures_and_no_replay() {
        for scopes in [
            vec![ApprovedScope::AppData],
            vec![ApprovedScope::File],
            vec![ApprovedScope::File, ApprovedScope::AppData],
            vec![ApprovedScope::AppData, ApprovedScope::File],
        ] {
            let (owner, ticket) = new_ticket(scopes).await;
            let original = ticket.url_for_native_launcher().unwrap().to_owned();
            assert_eq!(validate_destination(&original), Ok(()));
            let rig = Held::new();
            let marker = rig.dir.join("invoked");
            let mut command = Command::new("/usr/bin/python3");
            command.arg("-c").arg("import os,stat,sys\nn=os.stat(os.devnull)\nok=all(stat.S_ISCHR(os.fstat(fd).st_mode) and os.fstat(fd).st_rdev==n.st_rdev for fd in (0,1,2))\nwith open(sys.argv[1],'a') as f:f.write('one\\n')\nsys.exit(0 if ok else 19)\n").arg(&marker);
            assert_eq!(
                hand_off_with(ticket, synthetic(command, &rig.control)).await,
                Ok(LaunchOutcome::HandedOff)
            );
            assert!(rig.control.original.lock().unwrap().as_deref() == Some(original.as_str()));
            assert_eq!(std::fs::read_to_string(&marker).unwrap(), "one\n");
            assert_eq!(owner.snapshot().unwrap().phase, Phase::AwaitingCallback);
            assert!(owner.snapshot().unwrap().last_authorization.is_none());
            owner
                .cancel(owner.snapshot().unwrap().attempt_id.unwrap())
                .unwrap();
            let record = terminal(&owner).await;
            assert_eq!(record.outcome, AuthorizationOutcome::Cancelled);
            shutdown(&owner).await;
            assert_eq!(owner.snapshot().unwrap().last_authorization, Some(record));
        }
        for missing in [false, true] {
            let (owner, ticket) = new_ticket(vec![ApprovedScope::File]).await;
            let rig = Held::new();
            let marker = rig.dir.join("attempt");
            let command = if missing {
                Command::new(rig.dir.join("absent-selected-executable"))
            } else {
                let mut value = Command::new("/usr/bin/python3");
                value.arg("-c").arg("import sys\nwith open(sys.argv[1],'a') as f:f.write('one\\n')\nsys.exit(7)\n").arg(&marker);
                value
            };
            assert_eq!(
                hand_off_with(ticket, synthetic(command, &rig.control)).await,
                Err(LaunchError::LaunchFailed)
            );
            assert_eq!(
                terminal(&owner).await.outcome,
                AuthorizationOutcome::BrowserUnavailable
            );
            assert_eq!(rig.control.started.load(Ordering::Acquire), !missing);
            if missing {
                assert!(!marker.exists());
            } else {
                assert_eq!(std::fs::read_to_string(marker).unwrap(), "one\n");
            }
            shutdown(&owner).await;
        }
    }

    #[tokio::test]
    async fn independent_unpolled_stale_and_predispatch_cancellation_never_spawn() {
        let (owner, ticket) = new_ticket(vec![ApprovedScope::File]).await;
        drop(hand_off(ticket)); // The ONLY public-default use is unpolled.
        assert_eq!(
            terminal(&owner).await.outcome,
            AuthorizationOutcome::BrowserUnavailable
        );
        shutdown(&owner).await;
        let (owner, ticket) = new_ticket(vec![ApprovedScope::File]).await;
        let id = owner.snapshot().unwrap().attempt_id.unwrap();
        owner.cancel(id).unwrap();
        let rig = Held::new();
        assert_eq!(
            hand_off_with(ticket, rig.plan()).await,
            Err(LaunchError::Ticket(TicketError::Stale))
        );
        assert!(!rig.control.started.load(Ordering::Acquire));
        assert!(rig.control.original.lock().unwrap().is_none());
        assert!(!rig.dir.join("ready").exists());
        assert_eq!(
            terminal(&owner).await.outcome,
            AuthorizationOutcome::Cancelled
        );
        shutdown(&owner).await;
        for caller_drop in [false, true] {
            let (owner, ticket) = new_ticket(vec![ApprovedScope::File]).await;
            let rig = Held::new();
            rig.control.dispatch.store(false, Ordering::Release);
            let task = tokio::spawn(hand_off_with(ticket, rig.plan()));
            flag(&rig.control.entered).await;
            assert!(!rig.control.started.load(Ordering::Acquire));
            let expected = if caller_drop {
                task.abort();
                assert!(matches!(task.await,Err(e) if e.is_cancelled()));
                AuthorizationOutcome::BrowserUnavailable
            } else {
                owner
                    .cancel(owner.snapshot().unwrap().attempt_id.unwrap())
                    .unwrap();
                assert_eq!(
                    tokio::time::timeout(Duration::from_secs(3), task)
                        .await
                        .unwrap()
                        .unwrap(),
                    Err(LaunchError::Ticket(TicketError::Stale))
                );
                AuthorizationOutcome::Cancelled
            };
            assert_eq!(terminal(&owner).await.outcome, expected);
            assert!(!rig.control.started.load(Ordering::Acquire));
            assert!(!rig.dir.join("ready").exists());
            assert!(rig.control.original.lock().unwrap().is_none());
            shutdown(&owner).await;
        }
    }

    #[tokio::test]
    async fn independent_caller_future_drop_keeps_started_child_and_shutdown_fenced() {
        let (owner, ticket) = new_ticket(vec![ApprovedScope::File]).await;
        let rig = Held::new();
        let task = tokio::spawn(hand_off_with(ticket, rig.plan()));
        rig.started().await;
        task.abort();
        assert!(matches!(task.await,Err(e) if e.is_cancelled())); // Caller future only; blocking worker is not aborted.
        assert!(!rig.dir.join("done").exists());
        assert_eq!(owner.begin(both()).await.err(), Some(StartError::Busy));
        assert!(owner.snapshot().unwrap().last_authorization.is_none());
        assert!(
            tokio::time::timeout(Duration::from_millis(80), owner.shutdown())
                .await
                .is_err()
        );
        let record = owner.snapshot().unwrap().last_authorization.unwrap();
        assert_eq!(record.outcome, AuthorizationOutcome::Closed);
        assert_eq!(owner.snapshot().unwrap().phase, Phase::Draining);
        assert!(!rig.dir.join("done").exists());
        rig.release();
        shutdown(&owner).await;
        assert!(rig.dir.join("done").exists());
        assert_eq!(owner.snapshot().unwrap().last_authorization, Some(record));
    }

    #[tokio::test]
    async fn independent_controller_cancel_and_original_ten_second_deadline_hold_child() {
        for expire in [false, true] {
            let began = Instant::now();
            let (owner, ticket) = new_ticket(vec![ApprovedScope::File]).await;
            let rig = Held::new();
            let task = tokio::spawn(hand_off_with(ticket, rig.plan()));
            rig.started().await;
            let expected = if expire {
                AuthorizationOutcome::TimedOut
            } else {
                assert_eq!(
                    owner.cancel(owner.snapshot().unwrap().attempt_id.unwrap()),
                    Ok(CancelDisposition::Cancelled)
                );
                AuthorizationOutcome::Cancelled
            };
            let record = terminal(&owner).await;
            assert_eq!(record.outcome, expected);
            if expire {
                assert!(began.elapsed() >= Duration::from_secs(10));
            }
            assert_eq!(owner.snapshot().unwrap().phase, Phase::Draining);
            assert!(!task.is_finished());
            assert!(!rig.dir.join("done").exists());
            assert_eq!(owner.begin(both()).await.err(), Some(StartError::Busy));
            assert!(
                tokio::time::timeout(Duration::from_millis(80), owner.shutdown())
                    .await
                    .is_err()
            );
            assert_eq!(owner.snapshot().unwrap().last_authorization, Some(record));
            rig.release();
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(3), task)
                    .await
                    .unwrap()
                    .unwrap(),
                Err(LaunchError::Ticket(TicketError::Stale))
            );
            shutdown(&owner).await;
            assert_eq!(owner.snapshot().unwrap().last_authorization, Some(record));
        }
    }

    #[tokio::test]
    async fn independent_worker_unwind_and_exited_child_unknown_wait_keep_ticket() {
        for panic_worker in [false, true] {
            let (owner, ticket) = new_ticket(vec![ApprovedScope::File]).await;
            let rig = Held::new();
            rig.control
                .panic_after_spawn
                .store(panic_worker, Ordering::Release);
            rig.control
                .wait_error
                .store(!panic_worker, Ordering::Release);
            let task = tokio::spawn(hand_off_with(ticket, rig.plan()));
            rig.started().await;
            if !panic_worker {
                flag(&rig.control.wait_error_seen).await;
            }
            assert!(!task.is_finished());
            assert!(owner.snapshot().unwrap().last_authorization.is_none());
            assert_eq!(owner.begin(both()).await.err(), Some(StartError::Busy));
            rig.release();
            if !panic_worker {
                rig.exited_before_reap().await; // Real exited child, independent of injected flag.
                assert!(rig.control.wait_error.load(Ordering::Acquire));
                assert!(!task.is_finished());
                assert!(owner.snapshot().unwrap().last_authorization.is_none());
                assert_eq!(owner.begin(both()).await.err(), Some(StartError::Busy));
                rig.control.wait_error.store(false, Ordering::Release);
            }
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(3), task)
                    .await
                    .unwrap()
                    .unwrap(),
                Err(if panic_worker {
                    LaunchError::WorkerFailure
                } else {
                    LaunchError::LaunchFailed
                })
            );
            assert_eq!(
                terminal(&owner).await.outcome,
                AuthorizationOutcome::BrowserUnavailable
            );
            shutdown(&owner).await;
        }
    }

    fn denial_callback(original: &str) -> bool {
        let url = Url::parse(original).unwrap();
        let redirect = url
            .query_pairs()
            .find(|(k, _)| k == "redirect_uri")
            .unwrap()
            .1
            .into_owned();
        let state = url
            .query_pairs()
            .find(|(k, _)| k == "state")
            .unwrap()
            .1
            .into_owned();
        let redirect = Url::parse(&redirect).unwrap();
        assert!(redirect.host_str() == Some("127.0.0.1"));
        let port = redirect.port().unwrap();
        let target = format!("{}?state={state}&error=access_denied", redirect.path());
        std::thread::scope(|scope| {
            scope.spawn(move || {
            let address=SocketAddr::from((Ipv4Addr::LOCALHOST,port));
            let Ok(mut stream)=TcpStream::connect_timeout(&address,Duration::from_secs(1)) else{return false;};
            if stream.set_read_timeout(Some(Duration::from_millis(500))).is_err() || stream.set_write_timeout(Some(Duration::from_millis(500))).is_err(){return false;}
            if stream.write_all(format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n").as_bytes()).is_err(){return false;}
            let end=Instant::now()+Duration::from_secs(2);let mut reply=Vec::new();let mut buffer=[0;512];
            while Instant::now()<end && reply.len()<2048 {
                match stream.read(&mut buffer){Ok(0)=>break,Ok(n)=>{reply.extend_from_slice(&buffer[..n]);if reply.windows(4).any(|v|v==b"\r\n\r\n"){break;}},Err(_)=>return false}
            }
            reply.starts_with(b"HTTP/1.1 200 ")
        }).join().unwrap_or(false)
        })
    }
    #[tokio::test]
    async fn independent_local_denial_precedes_but_cannot_bypass_child_acknowledgment() {
        let (owner, ticket) = new_ticket(vec![ApprovedScope::File]).await;
        let original = ticket.url_for_native_launcher().unwrap().to_owned();
        let rig = Held::new();
        let task = tokio::spawn(hand_off_with(ticket, rig.plan()));
        rig.started().await;
        assert!(denial_callback(&original));
        assert_eq!(owner.snapshot().unwrap().phase, Phase::AwaitingBrowser);
        assert!(owner.snapshot().unwrap().last_authorization.is_none());
        assert!(!task.is_finished());
        assert_eq!(owner.begin(both()).await.err(), Some(StartError::Busy));
        rig.release();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(3), task)
                .await
                .unwrap()
                .unwrap(),
            Ok(LaunchOutcome::HandedOff)
        );
        let record = terminal(&owner).await;
        assert_eq!(record.outcome, AuthorizationOutcome::Denied);
        shutdown(&owner).await;
        assert_eq!(owner.snapshot().unwrap().last_authorization, Some(record));
        // Only access_denied: never a Code success or production exchange route.
    }
}
