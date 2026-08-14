//! Integration tests for the skeleton module.
//!
//! Replace this stub with real database-backed tests once you have
//! wired up a test harness. Typical setup:
//!
//! 1. Spin up a Postgres instance (docker, testcontainers, ...).
//! 2. Run the module migrations against it.
//! 3. Build `Module::builder().with_database(pool).build()?` and exercise
//!    `http_routes()` via `axum::http::Request` + `tower::ServiceExt`.

#[test]
fn skeleton_compiles() {
    // Placeholder test so `cargo test` at least has something to run
    // before the user wires up their real integration harness.
    assert_eq!(2 + 2, 4);
}
