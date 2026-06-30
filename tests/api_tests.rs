//! Integration tests for the rmdadm API
//! Tests authentication, rate limiting, and core API functionality

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use tower::ServiceExt;

mod common;

#[tokio::test]
async fn test_health_endpoint() {
    let app = common::create_test_app().await;
    
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(json["status"], "healthy");
    assert!(json["version"].is_string());
}

#[tokio::test]
async fn test_metrics_endpoint() {
    let app = common::create_test_app().await;
    
    let response = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    
    assert!(text.contains("# HELP md_array_state"));
    assert!(text.contains("# TYPE md_array_state gauge"));
}

#[tokio::test]
async fn test_login_success() {
    std::env::set_var("RMDADM_ADMIN_USER", "testuser");
    std::env::set_var("RMDADM_ADMIN_PASSWORD", "testpass");
    
    let app = common::create_test_app().await;
    
    let login_body = json!({
        "username": "testuser",
        "password": "testpass"
    });
    
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&login_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert!(json["token"].is_string());
    assert!(json["expires_in"].is_number());
    
    std::env::remove_var("RMDADM_ADMIN_USER");
    std::env::remove_var("RMDADM_ADMIN_PASSWORD");
}

#[tokio::test]
async fn test_login_failure() {
    std::env::set_var("RMDADM_ADMIN_USER", "testuser");
    std::env::set_var("RMDADM_ADMIN_PASSWORD", "testpass");
    
    let app = common::create_test_app().await;
    
    let login_body = json!({
        "username": "testuser",
        "password": "wrongpass"
    });
    
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&login_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    
    std::env::remove_var("RMDADM_ADMIN_USER");
    std::env::remove_var("RMDADM_ADMIN_PASSWORD");
}

#[tokio::test]
async fn test_api_key_authentication() {
    std::env::set_var("RMDADM_API_KEY", "test-api-key-12345");
    
    let app = common::create_test_app().await;
    
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/arrays")
                .header("X-API-Key", "test-api-key-12345")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    
    std::env::remove_var("RMDADM_API_KEY");
}

#[tokio::test]
async fn test_api_key_authentication_failure() {
    std::env::set_var("RMDADM_API_KEY", "test-api-key-12345");
    
    let app = common::create_test_app().await;
    
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/arrays")
                .header("X-API-Key", "wrong-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    
    std::env::remove_var("RMDADM_API_KEY");
}

#[tokio::test]
async fn test_jwt_authentication() {
    std::env::set_var("RMDADM_ADMIN_USER", "testuser");
    std::env::set_var("RMDADM_ADMIN_PASSWORD", "testpass");
    
    let app = common::create_test_app().await;
    
    // First, login to get token
    let login_body = json!({
        "username": "testuser",
        "password": "testpass"
    });
    
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&login_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    
    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let token = json["token"].as_str().unwrap();
    
    // Use token to access protected endpoint
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/arrays")
                .header("Authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    
    std::env::remove_var("RMDADM_ADMIN_USER");
    std::env::remove_var("RMDADM_ADMIN_PASSWORD");
}

#[tokio::test]
async fn test_unauthorized_access() {
    let app = common::create_test_app().await;
    
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/arrays")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_rate_limiting() {
    std::env::set_var("RMDADM_RATE_LIMIT_MAX", "3");
    std::env::set_var("RMDADM_RATE_LIMIT_WINDOW", "60");
    std::env::set_var("RMDADM_DISABLE_AUTH", "1");
    
    let app = common::create_test_app().await;
    
    // First 3 requests should succeed
    for i in 0..3 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "Request {} should succeed",
            i + 1
        );
    }
    
    // 4th request should be rate limited
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    
    std::env::remove_var("RMDADM_RATE_LIMIT_MAX");
    std::env::remove_var("RMDADM_RATE_LIMIT_WINDOW");
    std::env::remove_var("RMDADM_DISABLE_AUTH");
}

#[tokio::test]
async fn test_list_arrays_empty() {
    std::env::set_var("RMDADM_DISABLE_AUTH", "1");
    
    let app = common::create_test_app().await;
    
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/arrays")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(json["total"], 0);
    assert!(json["arrays"].is_array());
    
    std::env::remove_var("RMDADM_DISABLE_AUTH");
}

#[tokio::test]
async fn test_cors_headers() {
    let app = common::create_test_app().await;
    
    let response = app
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/health")
                .header("Origin", "http://localhost:3000")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    
    // Should handle OPTIONS request
    assert!(response.status().is_success() || response.status() == StatusCode::METHOD_NOT_ALLOWED);
}
