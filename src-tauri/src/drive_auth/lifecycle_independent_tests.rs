use super::*;
use std::sync::Barrier;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const BEGIN: &str =
    r#"{"schemaVersion":1,"registrationId":"fixture","capabilities":["stateLibrary"]}"#;
const DECIDE: &str = r#"{"schemaVersion":1,"attemptId":"00000000-0000-0000-0000-000000000000","generation":"0","decisionId":"00000000-0000-0000-0000-000000000000","decision":"accept"}"#;

fn poison_open(owner: &DesktopState) {
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _held = owner.coordinator.state.lock().unwrap();
            panic!("independent controlled Open poison");
        }))
        .is_err()
    );
}
fn complete() -> CleanupResult {
    CleanupResult::CleanupComplete { fault: None }
}
fn admitted(intent: Intent) -> DesktopState {
    let owner = DesktopState::empty().unwrap();
    assert!(owner.prepare_request(intent).unwrap().start);
    let closing = owner.application.mark_closing();
    owner.coordinator.closure_fault(closing.fault);
    owner
}
fn closed_without_work(owner: &DesktopState) {
    assert_eq!(
        futures::executor::block_on(owner.application.begin(BEGIN)),
        Err(FacadeError::Closing)
    );
    assert_eq!(owner.application.decide(DECIDE), Err(FacadeError::Closing));
    assert!(!owner.application.mark_closing().first);
    let inner = owner.coordinator.lock();
    assert_eq!(inner.snapshot.phase, Phase::Blocked);
    assert!(!inner.fence);
    assert!(!inner.queued);
}

#[test]
fn independent_fatal_first_observation_closes_the_real_owner() {
    // Each owner is poisoned before ANY snapshot/marker/admission observation.
    for observer in 0..5 {
        let owner = DesktopState::empty().unwrap();
        poison_open(&owner);
        match observer {
            0 => assert!(matches!(
                owner.prepare_request(Intent::Restart),
                Err(RequestError::Unavailable)
            )),
            1 => assert_eq!(owner.snapshot().phase, Phase::Blocked),
            2 => assert_eq!(owner.prepare_exit_response(None), ExitResponse::Prevent),
            3 => assert_eq!(owner.prepare_exit_response(Some(0)), ExitResponse::Prevent),
            _ => assert_eq!(owner.prepare_exit_response(Some(1)), ExitResponse::Prevent),
        }
        closed_without_work(&owner);
        assert_eq!(owner.snapshot().fault, Some(Fault::Poisoned));
        assert_eq!(owner.snapshot().closure_fault, None);
        assert_eq!(owner.snapshot().intent, None);
        assert!(matches!(
            owner.prepare_request(Intent::Quit { code: 7 }),
            Err(RequestError::Unavailable)
        ));
        assert!(!owner.coordinator.cleanup(complete()));
        assert!(owner.coordinator.seal().is_none());
        closed_without_work(&owner);
    }
    let clean = DesktopState::empty().unwrap();
    assert!(matches!(
        clean.prepare_request(Intent::Quit {
            code: tauri::RESTART_EXIT_CODE
        }),
        Err(RequestError::Unavailable)
    ));
    assert_eq!(clean.snapshot().phase, Phase::Open);
    assert_eq!(
        futures::executor::block_on(clean.application.begin(BEGIN)),
        Err(FacadeError::NotConfigured)
    );
    // Reconciliation must not hold the coordinator mutex during A closure.
    let text = include_str!("lifecycle.rs");
    assert!(text.contains("let observed = self.coordinator.snapshot();"));
    assert!(text.contains("let closing = self.application.mark_closing();"));
}

#[test]
fn independent_first_code_and_mutex_seal_winners_are_fixed() {
    for code in [i32::MIN, -1, 0, 1, 77, i32::MAX - 1] {
        let owner = admitted(Intent::Restart);
        assert_eq!(
            owner.prepare_exit_response(Some(code)),
            ExitResponse::RequestQuit(code)
        );
        let first = owner.prepare_request(Intent::Quit { code }).unwrap();
        assert!(!first.start);
        assert_eq!(first.acknowledgment, Acknowledgment::Accepted);
        assert!(
            !owner
                .prepare_request(Intent::Quit { code: 0 })
                .unwrap()
                .start
        );
        assert_eq!(
            owner
                .prepare_request(Intent::Restart)
                .unwrap()
                .acknowledgment,
            Acknowledgment::QuitPending
        );
        assert_eq!(owner.snapshot().intent, Some(Intent::Quit { code }));
        assert!(owner.coordinator.seal().is_none());
    }
    let none = DesktopState::empty().unwrap();
    assert_eq!(
        none.prepare_exit_response(None),
        ExitResponse::RequestQuit(0)
    );
    none.prepare_request(Intent::Quit { code: 0 }).unwrap();
    assert_eq!(
        none.prepare_request(Intent::Restart)
            .unwrap()
            .acknowledgment,
        Acknowledgment::QuitPending
    );
    for quit_before_seal in [true, false] {
        let owner = admitted(Intent::Restart);
        let c = Arc::clone(&owner.coordinator);
        let count = Arc::new(AtomicUsize::new(0));
        let result = Arc::new(Mutex::new(None));
        let action_count = Arc::clone(&count);
        let action_result = Arc::clone(&result);
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        futures::executor::block_on(drain_and_queue(
            RunnerGuard::new(Arc::clone(&c)),
            async { complete() },
            move |f| {
                send.send(f).unwrap();
                Ok(())
            },
            move |intent| {
                *action_result.lock().unwrap() = Some(intent);
                action_count.fetch_add(1, Ordering::SeqCst);
            },
        ));
        assert_eq!(count.load(Ordering::SeqCst), 0);
        assert_eq!(owner.snapshot().phase, Phase::Draining);
        if quit_before_seal {
            owner.prepare_request(Intent::Quit { code: 1 }).unwrap();
            owner.prepare_request(Intent::Quit { code: 0 }).unwrap();
        }
        receive.recv_timeout(Duration::from_secs(1)).unwrap()();
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(
            *result.lock().unwrap(),
            Some(if quit_before_seal {
                Intent::Quit { code: 1 }
            } else {
                Intent::Restart
            })
        );
        assert!(matches!(
            owner.prepare_request(Intent::Quit { code: 0 }),
            Err(RequestError::AlreadyFinalizing)
        ));
        assert!(c.seal().is_none());
    }
    // Two genuinely concurrent seals can have exactly one winner.
    let c = Arc::new(Coordinator::new());
    c.request(Intent::Restart).unwrap();
    assert!(c.cleanup(complete()));
    let barrier = Arc::new(Barrier::new(3));
    let mut workers = Vec::new();
    for _ in 0..2 {
        let c = Arc::clone(&c);
        let barrier = Arc::clone(&barrier);
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            c.seal()
        }));
    }
    barrier.wait();
    let wins = workers
        .into_iter()
        .map(|w| w.join().unwrap())
        .filter(Option::is_some)
        .count();
    assert_eq!(wins, 1);
}

#[tokio::test]
async fn independent_guard_and_cleanup_failures_never_authorize_retry() {
    for polled in [false, true] {
        let owner = admitted(Intent::Restart);
        let mut runner = Box::pin(drain_and_queue(
            RunnerGuard::new(Arc::clone(&owner.coordinator)),
            std::future::pending(),
            |_| panic!("independent unexpected queue"),
            |_| panic!("independent unexpected action"),
        ));
        if polled {
            assert!(futures::poll!(&mut runner).is_pending());
        }
        drop(runner);
        assert_eq!(
            owner.snapshot().fault,
            Some(if polled {
                Fault::RunnerFailed
            } else {
                Fault::SpawnFailed
            })
        );
        closed_without_work(&owner);
        assert!(matches!(
            owner.prepare_request(Intent::Restart),
            Err(RequestError::Unavailable)
        ));
    }
    for failure in 0..3 {
        let owner = admitted(Intent::Quit { code: 1 });
        let c = Arc::clone(&owner.coordinator);
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            futures::executor::block_on(drain_and_queue(
                RunnerGuard::new(Arc::clone(&c)),
                async move {
                    if failure == 0 {
                        panic!("independent cleanup panic");
                    }
                    complete()
                },
                move |_| {
                    if failure == 2 {
                        panic!("independent queue panic");
                    }
                    Err(())
                },
                |_| panic!("independent unexpected action"),
            ))
        }));
        assert_eq!(outcome.is_err(), failure != 1);
        assert_eq!(
            owner.snapshot().fault,
            Some(if failure == 0 {
                Fault::RunnerFailed
            } else {
                Fault::ScheduleFailed
            })
        );
        assert!(c.seal().is_none());
        assert!(matches!(
            owner.prepare_request(Intent::Restart),
            Err(RequestError::Unavailable)
        ));
    }
    let owner = admitted(Intent::Restart);
    // Linked healthy A is authoritative: no A fault was produced here.
    drain_and_queue(
        RunnerGuard::new(Arc::clone(&owner.coordinator)),
        async {
            CleanupResult::CleanupUnproven {
                fault: FacadeError::Closed,
            }
        },
        |_| panic!("independent unproven queue"),
        |_| panic!("independent unproven action"),
    )
    .await;
    assert_eq!(owner.snapshot().closure_fault, None);
    assert_eq!(
        owner.snapshot().fault,
        Some(Fault::CleanupUnproven(FacadeError::Closed))
    );
    assert!(owner.coordinator.seal().is_none());

    // PURE Coordinator bookkeeping, with no linked A or invented A fault.
    let pure = Coordinator::new();
    assert!(pure.request(Intent::Restart).unwrap().start);
    pure.closure_fault(Some(FacadeError::ControllerFault));
    assert!(!pure.cleanup(CleanupResult::CleanupUnproven {
        fault: FacadeError::Closed,
    }));
    assert_eq!(
        pure.snapshot().closure_fault,
        Some(FacadeError::ControllerFault)
    );
    assert_eq!(
        pure.snapshot().fault,
        Some(Fault::CleanupUnproven(FacadeError::Closed))
    );
    assert!(pure.seal().is_none());

    let owner = admitted(Intent::Restart);
    drain_and_queue(
        RunnerGuard::new(Arc::clone(&owner.coordinator)),
        async { complete() },
        |never_run| {
            drop(never_run);
            Ok(())
        },
        |_| panic!("independent never-run action"),
    )
    .await;
    assert_eq!(owner.snapshot().phase, Phase::Draining);
    assert!(!owner.application.mark_closing().first);
}

#[tokio::test]
async fn independent_real_cleanup_fences_precede_finalizer_queue() {
    let owner = admitted(Intent::Quit { code: 0 });
    let weak = Arc::downgrade(&owner.application);
    let application = Arc::clone(&owner.application);
    let count = Arc::new(AtomicUsize::new(0));
    let action_count = Arc::clone(&count);
    drain_and_queue(
        RunnerGuard::new(Arc::clone(&owner.coordinator)),
        async move { application.shutdown().await },
        |f| {
            f();
            Ok(())
        },
        move |intent| {
            assert_eq!(intent, Intent::Quit { code: 0 });
            action_count.fetch_add(1, Ordering::SeqCst);
        },
    )
    .await;
    assert_eq!(count.load(Ordering::SeqCst), 1);
    drop(owner);
    assert!(weak.upgrade().is_none());
    // Independent public SessionController fixture, NOT held-ticket A integration.
    use super::super::pkce::ApprovedScope;
    use super::super::session::{NativeStart, SessionController};
    let controller = Arc::new(SessionController::new());
    let ticket = tokio::time::timeout(
        Duration::from_secs(4),
        controller.begin(
            NativeStart::new(
                "fixture.apps.googleusercontent.com".into(),
                "fixture".into(),
                "1".into(),
                vec![ApprovedScope::AppData],
            )
            .unwrap(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let owner = admitted(Intent::Restart);
    let native = Arc::clone(&controller);
    let queues = Arc::new(AtomicUsize::new(0));
    let q = Arc::clone(&queues);
    let mut runner = Box::pin(drain_and_queue(
        RunnerGuard::new(Arc::clone(&owner.coordinator)),
        async move {
            native
                .shutdown()
                .await
                .expect("independent native cleanup fence");
            complete()
        },
        move |f| {
            q.fetch_add(1, Ordering::SeqCst);
            f();
            Ok(())
        },
        |_| {},
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(40), &mut runner)
            .await
            .is_err()
    );
    assert_eq!(queues.load(Ordering::SeqCst), 0);
    assert_eq!(owner.snapshot().phase, Phase::Draining);
    drop(ticket); // failed handoff only, NEVER a successful Code acknowledgment.
    tokio::time::timeout(Duration::from_secs(4), runner)
        .await
        .unwrap();
    assert_eq!(queues.load(Ordering::SeqCst), 1);
    assert_eq!(owner.snapshot().phase, Phase::Finalizing);
}

#[test]
fn independent_recursive_exit_boundary_and_source_action_order() {
    for intent in [
        Intent::Restart,
        Intent::Quit { code: 1 },
        Intent::Quit { code: -8 },
    ] {
        let owner = admitted(intent);
        let expected = match intent {
            Intent::Restart => tauri::RESTART_EXIT_CODE,
            Intent::Quit { code } => code,
        };
        assert_ne!(
            owner.prepare_exit_response(Some(expected)),
            ExitResponse::Allow
        );
        owner
            .coordinator
            .closure_fault(Some(FacadeError::ControllerFault));
        assert!(owner.coordinator.cleanup(CleanupResult::CleanupComplete {
            fault: Some(FacadeError::WorkerFailure)
        }));
        assert_eq!(owner.coordinator.seal(), Some(intent));
        assert_eq!(
            owner.prepare_exit_response(Some(expected)),
            ExitResponse::Allow
        );
        assert_eq!(owner.prepare_exit_response(None), ExitResponse::Prevent);
        assert_ne!(
            owner.prepare_exit_response(Some(expected.wrapping_add(1))),
            ExitResponse::Allow
        );
        assert_eq!(
            owner.snapshot().closure_fault,
            Some(FacadeError::ControllerFault)
        );
        assert_eq!(
            owner.snapshot().cleanup_fault,
            Some(FacadeError::WorkerFailure)
        );
    }
    let owner = DesktopState::empty().unwrap();
    assert_eq!(
        owner.prepare_exit_response(Some(tauri::RESTART_EXIT_CODE)),
        ExitResponse::ForeignReserved
    );
    let source = include_str!("lifecycle.rs");
    let production = source.split("#[cfg(test)]").next().unwrap();
    assert!(!production.contains("std::process::exit("));
    assert!(!production.contains("actor.restart("));
    assert!(production.contains("actor.request_restart()"));
    assert!(
        production
            .contains("if let Some(intent) = coordinator.seal() {\n            action(intent);")
    );
    let request = production.split("pub(super) fn request(").nth(1).unwrap();
    assert!(
        request.find("RunnerGuard::new").unwrap()
            < request.find("tauri::async_runtime::spawn").unwrap()
    );
    let lib = include_str!("../lib.rs");
    assert!(
        lib.find("DesktopState::empty()").unwrap()
            < lib.find(".build(tauri::generate_context!())").unwrap()
    );
    assert!(
        lib.contains("drive_auth::lifecycle::on_exit_requested(&state, app_handle, code, &api);")
    );
    assert!(lib.contains("drive_auth::lifecycle::on_exit(&state);"));
}
