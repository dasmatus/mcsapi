use mcsapi_mcp::{
    ArrangeRequest, LayoutName, Operation, PlacementSnapshot, Rect, SimulateRequest, arrange,
    simulate,
};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

const BOUNDS: Rect = Rect {
    x: 0,
    y: 0,
    width: 1920,
    height: 1080,
};

#[test]
fn oversized_requests_are_refused() {
    let many = mcsapi_mcp::MAX_ITEMS + 1;
    // The exact message, so an error from later in the call (a layout out of
    // space, a duplicate ID) cannot pass for the limit.
    let refused = |what: &str| {
        format!(
            "too many {what}: {many}, the limit is {}",
            mcsapi_mcp::MAX_ITEMS
        )
    };
    let error = simulate(&SimulateRequest {
        workspaces: (1..=many as u64).collect(),
        operations: Vec::new(),
        bounds: None,
    })
    .unwrap_err();
    assert_eq!(error, refused("workspaces"));
    let error = simulate(&SimulateRequest {
        workspaces: vec![1],
        operations: vec![Operation::FocusNext; many],
        bounds: None,
    })
    .unwrap_err();
    assert_eq!(error, refused("operations"));
    let error = arrange(&ArrangeRequest {
        layout: LayoutName::Monocle,
        bounds: BOUNDS,
        windows: (1..=many as u64).collect(),
    })
    .unwrap_err();
    assert_eq!(error, refused("windows"));
}

#[test]
fn simulate_replays_operations_on_a_fresh_desktop() {
    let snapshot = simulate(&SimulateRequest {
        workspaces: vec![1, 2],
        operations: vec![
            Operation::Insert { window: 1 },
            Operation::Insert { window: 2 },
            Operation::Insert { window: 3 },
            Operation::FocusPrevious,
            Operation::PromoteFocused,
            Operation::MoveWindow {
                window: 3,
                workspace: 2,
            },
        ],
        bounds: Some(BOUNDS),
    })
    .unwrap();

    assert_eq!(snapshot.active, 1);
    assert_eq!(snapshot.workspaces[0].windows, [2, 1]);
    assert_eq!(snapshot.workspaces[0].focused, Some(2));
    assert_eq!(snapshot.workspaces[1].windows, [3]);
    assert_eq!(
        snapshot.placements.unwrap()[0],
        PlacementSnapshot {
            window: 2,
            geometry: Rect {
                width: 960,
                ..BOUNDS
            },
        }
    );
}

#[test]
fn simulate_is_stateless() {
    let request = SimulateRequest {
        workspaces: vec![1],
        operations: vec![Operation::Insert { window: 1 }],
        bounds: None,
    };
    assert_eq!(simulate(&request), simulate(&request));
}

#[test]
fn simulate_names_the_failing_operation() {
    let error = simulate(&SimulateRequest {
        workspaces: vec![1],
        operations: vec![
            Operation::Insert { window: 1 },
            Operation::Insert { window: 1 },
        ],
        bounds: None,
    })
    .unwrap_err();
    assert!(error.starts_with("operation 1 "), "{error}");
    assert!(error.ends_with("duplicate window: 1"), "{error}");
}

#[test]
fn simulate_rejects_zero_and_missing_workspaces() {
    let zero = SimulateRequest {
        workspaces: vec![0],
        operations: vec![],
        bounds: None,
    };
    assert!(simulate(&zero).is_err());
    let none = SimulateRequest {
        workspaces: vec![],
        ..zero
    };
    assert_eq!(
        simulate(&none).unwrap_err(),
        "at least one workspace is required"
    );
}

#[test]
fn operations_deserialize_from_tagged_json() {
    let request: SimulateRequest = serde_json::from_value(json!({
        "workspaces": [1],
        "operations": [
            {"op": "insert", "window": 4},
            {"op": "set_layout", "layout": "monocle"},
            {"op": "focus_next"}
        ]
    }))
    .unwrap();
    let snapshot = simulate(&request).unwrap();
    assert_eq!(snapshot.workspaces[0].layout, "monocle");
    assert!(snapshot.placements.is_none());
}

#[test]
fn arrange_monocle_and_tiny_outputs() {
    let monocle = arrange(&ArrangeRequest {
        layout: LayoutName::Monocle,
        bounds: BOUNDS,
        windows: vec![1, 2],
    })
    .unwrap();
    assert!(monocle.placements.iter().all(|p| p.geometry == BOUNDS));

    let tiny = arrange(&ArrangeRequest {
        layout: LayoutName::Tall,
        bounds: Rect { width: 1, ..BOUNDS },
        windows: vec![1, 2],
    });
    assert_eq!(tiny.unwrap_err(), "not enough space for tiled windows");
}

async fn post(addr: std::net::SocketAddr, body: Value) -> Value {
    let body = body.to_string();
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let request = format!(
        "POST /mcp HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\nMCP-Protocol-Version: 2025-06-18\r\n\
         Connection: close\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert!(
        !head.to_ascii_lowercase().contains("mcp-session-id"),
        "{head}"
    );
    serde_json::from_str(body).unwrap()
}

#[tokio::test]
async fn http_calls_need_no_session() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let shutdown = CancellationToken::new();
    let server =
        tokio::spawn(axum::serve(listener, mcsapi_mcp::router(shutdown.clone())).into_future());

    // No initialize: each request stands alone.
    let tools = post(
        addr,
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
    )
    .await;
    let names: Vec<_> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["arrange", "simulate"]);

    let call = post(
        addr,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "arrange",
                "arguments": {"layout": "tall", "bounds": BOUNDS, "windows": [1]}
            }
        }),
    )
    .await;
    assert_eq!(
        call["result"]["structuredContent"]["placements"][0]["geometry"],
        json!(BOUNDS)
    );

    shutdown.cancel();
    server.abort();
}
