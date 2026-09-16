use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    thread,
};

use ores_api_docs_client::{PageContext, PageState, PrerenderContext};

#[derive(Debug)]
struct RpcPool {
    endpoint: String,
    calls: AtomicUsize,
}

#[derive(Debug)]
struct AppState {
    rpc: Arc<RpcPool>,
    tenant: String,
}

fn main() {
    let rpc = Arc::new(RpcPool {
        endpoint: "http://127.0.0.1:9123".to_owned(),
        calls: AtomicUsize::new(0),
    });
    let app = AppState {
        rpc: Arc::clone(&rpc),
        tenant: "shared-auth-test".to_owned(),
    };
    let ctx = PageContext::with_state(
        BTreeMap::from([("id".to_owned(), "alice".to_owned())]),
        "/users/alice",
        app,
    );

    assert_eq!(ctx.route_params.get("id").map(String::as_str), Some("alice"));
    assert_eq!(ctx.request_path, "/users/alice");
    assert!(!ctx.state.is_empty());
    assert!(ctx.state::<String>().is_none(), "wrong-type recovery must fail closed");

    let recovered = ctx.state::<AppState>().expect("typed app state");
    assert_eq!(recovered.tenant, "shared-auth-test");
    assert_eq!(recovered.rpc.endpoint, "http://127.0.0.1:9123");
    assert!(Arc::ptr_eq(&recovered.rpc, &rpc));

    let mut workers = Vec::new();
    for _ in 0..16 {
        let cloned = ctx.clone();
        workers.push(thread::spawn(move || {
            let state = cloned.state::<AppState>().expect("state survives PageContext clone");
            state.rpc.calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(state.tenant, "shared-auth-test");
        }));
    }
    for worker in workers {
        worker.join().expect("concurrent page worker");
    }
    assert_eq!(rpc.calls.load(Ordering::SeqCst), 16);

    let empty = PageContext::new(BTreeMap::new(), "/public");
    assert!(empty.state.is_empty());
    assert!(empty.state::<AppState>().is_none());

    let prerender = PrerenderContext {
        route_manifest_sha256: "a".repeat(64),
        rpc_contract_sha256: Some("b".repeat(64)),
        source_version: Some("snapshot-2026-09-16".to_owned()),
        state: PageState::new(Arc::clone(&rpc)),
    };
    let prerender_rpc = prerender
        .state::<Arc<RpcPool>>()
        .expect("typed deterministic build state");
    assert!(Arc::ptr_eq(prerender_rpc.as_ref(), &rpc));
    assert_eq!(prerender.source_version.as_deref(), Some("snapshot-2026-09-16"));

    println!("shared-auth-test ores page state ABI smoke passed");
}
