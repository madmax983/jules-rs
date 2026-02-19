use jules_rs::{
    AutomationMode, CreateSessionRequest, JulesClient, JulesError, ListSourcesParams, RetryPolicy,
    SessionGithubRepoContext, SourceContext, TimeoutPolicy,
};
use serde_json::json;
use std::time::Duration;
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn list_sources_includes_query_params_and_parses_response() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1alpha/sources"))
        .and(header("x-goog-api-key", "test-api-key"))
        .and(query_param("pageSize", "25"))
        .and(query_param("pageToken", "cursor-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sources": [
                {
                    "name": "sources/demo",
                    "displayName": "Demo",
                    "githubRepo": {
                        "owner": "acme",
                        "repo": "widgets"
                    }
                }
            ],
            "nextPageToken": "cursor-2"
        })))
        .mount(&server)
        .await;

    let client = JulesClient::builder("test-api-key")
        .base_url(format!("{}/v1alpha", server.uri()))
        .build()
        .expect("build test client");

    let response = client
        .list_sources(ListSourcesParams {
            page_size: Some(25),
            page_token: Some("cursor-1".to_string()),
        })
        .await
        .expect("list sources response");

    assert_eq!(response.sources.len(), 1);
    assert_eq!(response.sources[0].name, "sources/demo");
    assert_eq!(response.next_page_token.as_deref(), Some("cursor-2"));
}

#[tokio::test]
async fn create_session_serializes_request_and_parses_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1alpha/sessions"))
        .and(header("x-goog-api-key", "test-api-key"))
        .and(body_json(json!({
            "prompt": "Add logging",
            "title": "Logging patch",
            "sourceContext": {
                "source": "sources/demo",
                "githubRepoContext": { "startingBranch": "main" }
            },
            "requirePlanApproval": true,
            "automationMode": "AUTO_CREATE_PR"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "sessions/s-1",
            "id": "s-1",
            "title": "Logging patch",
            "state": "AWAITING_PLAN_APPROVAL",
            "sourceContext": {
                "source": "sources/demo",
                "githubRepoContext": { "startingBranch": "main" }
            },
            "plan": {
                "steps": [
                    {"description": "Add log line", "status": "PENDING"}
                ]
            }
        })))
        .mount(&server)
        .await;

    let client = JulesClient::builder("test-api-key")
        .base_url(format!("{}/v1alpha", server.uri()))
        .build()
        .expect("build test client");

    let response = client
        .create_session(CreateSessionRequest {
            prompt: "Add logging".to_string(),
            title: Some("Logging patch".to_string()),
            source_context: Some(SourceContext {
                source: "sources/demo".to_string(),
                github_repo_context: Some(SessionGithubRepoContext {
                    starting_branch: "main".to_string(),
                }),
            }),
            require_plan_approval: Some(true),
            automation_mode: Some(AutomationMode::AutoCreatePr),
        })
        .await
        .expect("create session response");

    assert_eq!(response.name, "sessions/s-1");
    assert_eq!(response.title.as_deref(), Some("Logging patch"));
    assert_eq!(
        response.source_context.as_ref().map(|c| c.source.as_str()),
        Some("sources/demo")
    );
}

#[tokio::test]
async fn send_message_accepts_empty_success_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1alpha/sessions/s-1:sendMessage"))
        .and(header("x-goog-api-key", "test-api-key"))
        .and(body_json(json!({ "prompt": "Ship it" })))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let client = JulesClient::builder("test-api-key")
        .base_url(format!("{}/v1alpha", server.uri()))
        .build()
        .expect("build test client");

    client
        .send_message("s-1", "Ship it")
        .await
        .expect("send message succeeds");
}

#[tokio::test]
async fn non_success_responses_are_mapped_to_api_errors() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1alpha/sessions/missing"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "error": {
                "code": 404,
                "message": "session not found",
                "status": "NOT_FOUND"
            }
        })))
        .mount(&server)
        .await;

    let client = JulesClient::builder("test-api-key")
        .base_url(format!("{}/v1alpha", server.uri()))
        .build()
        .expect("build test client");

    let error = client
        .get_session("missing")
        .await
        .expect_err("expected api error");

    match error {
        JulesError::Api(api_error) => {
            assert_eq!(api_error.status_code, 404);
            assert_eq!(api_error.code, Some(404));
            assert_eq!(api_error.status.as_deref(), Some("NOT_FOUND"));
            assert_eq!(api_error.message, "session not found");
        }
        other => panic!("expected JulesError::Api, got {other:?}"),
    }
}

#[tokio::test]
async fn invalid_page_size_is_rejected_before_request() {
    let server = MockServer::start().await;
    let client = JulesClient::builder("test-api-key")
        .base_url(format!("{}/v1alpha", server.uri()))
        .build()
        .expect("build test client");

    let error = client
        .list_sources(ListSourcesParams {
            page_size: Some(0),
            page_token: None,
        })
        .await
        .expect_err("expected validation error");

    match error {
        JulesError::InvalidArgument(message) => {
            assert!(
                message.contains("page_size"),
                "validation message should mention page_size"
            );
        }
        other => panic!("expected JulesError::InvalidArgument, got {other:?}"),
    }
}

#[tokio::test]
async fn retry_policy_retries_transient_status_codes() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1alpha/sources"))
        .respond_with(ResponseTemplate::new(503).set_body_json(json!({
            "error": {
                "code": 503,
                "message": "temporary outage",
                "status": "UNAVAILABLE"
            }
        })))
        .mount(&server)
        .await;

    let client = JulesClient::builder("test-api-key")
        .base_url(format!("{}/v1alpha", server.uri()))
        .retry_policy(RetryPolicy {
            max_retries: 2,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(1),
        })
        .build()
        .expect("build test client");

    let error = client
        .list_sources(ListSourcesParams::default())
        .await
        .expect_err("expected retryable api error");
    assert!(matches!(error, JulesError::Api(_)));

    let requests = server
        .received_requests()
        .await
        .expect("read received requests");
    assert_eq!(requests.len(), 3, "initial request plus two retries");
}

#[tokio::test]
async fn timeout_policy_enforces_deadline() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1alpha/sources"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(250))
                .set_body_json(json!({ "sources": [] })),
        )
        .mount(&server)
        .await;

    let client = JulesClient::builder("test-api-key")
        .base_url(format!("{}/v1alpha", server.uri()))
        .timeout_policy(TimeoutPolicy {
            request_timeout: Duration::from_millis(20),
        })
        .retry_policy(RetryPolicy {
            max_retries: 0,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(1),
        })
        .build()
        .expect("build test client");

    let error = client
        .list_sources(ListSourcesParams::default())
        .await
        .expect_err("expected timeout");

    match error {
        JulesError::Http(http_error) => assert!(http_error.is_timeout()),
        other => panic!("expected timeout http error, got {other:?}"),
    }
}

#[tokio::test]
async fn get_source_accepts_nested_github_resource_names() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1alpha/sources/github/madmax983/force-rs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "sources/github/madmax983/force-rs",
            "githubRepo": {
                "owner": "madmax983",
                "repo": "force-rs",
                "defaultBranch": {
                    "displayName": "main"
                }
            }
        })))
        .mount(&server)
        .await;

    let client = JulesClient::builder("test-api-key")
        .base_url(format!("{}/v1alpha", server.uri()))
        .build()
        .expect("build test client");

    let source = client
        .get_source("sources/github/madmax983/force-rs")
        .await
        .expect("source lookup succeeds");

    assert_eq!(source.name, "sources/github/madmax983/force-rs");
    assert_eq!(
        source
            .github_repo
            .and_then(|repo| repo.default_branch.map(|branch| branch.display_name))
            .as_deref(),
        Some("main")
    );
}

#[tokio::test]
async fn create_session_does_not_retry_on_retryable_status_codes() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1alpha/sessions"))
        .respond_with(ResponseTemplate::new(503).set_body_json(json!({
            "error": {
                "code": 503,
                "message": "temporary outage",
                "status": "UNAVAILABLE"
            }
        })))
        .mount(&server)
        .await;

    let client = JulesClient::builder("test-api-key")
        .base_url(format!("{}/v1alpha", server.uri()))
        .retry_policy(RetryPolicy {
            max_retries: 3,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(1),
        })
        .build()
        .expect("build test client");

    let error = client
        .create_session(CreateSessionRequest {
            prompt: "hello".to_string(),
            title: None,
            source_context: None,
            require_plan_approval: None,
            automation_mode: None,
        })
        .await
        .expect_err("expected api error");
    assert!(matches!(error, JulesError::Api(_)));

    let requests = server
        .received_requests()
        .await
        .expect("read received requests");
    assert_eq!(requests.len(), 1, "non-idempotent POST should not retry");
}

#[tokio::test]
async fn debug_output_redacts_api_key_for_builder_and_client() {
    let builder =
        JulesClient::builder("super-secret-api-key").base_url("http://localhost:8080/v1alpha");
    let builder_debug = format!("{builder:?}");
    assert!(
        !builder_debug.contains("super-secret-api-key"),
        "builder debug output must not leak api key"
    );

    let client = builder.build().expect("build client for debug");
    let client_debug = format!("{client:?}");
    assert!(
        !client_debug.contains("super-secret-api-key"),
        "client debug output must not leak api key"
    );
}
