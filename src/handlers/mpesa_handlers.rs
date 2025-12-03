// handlers/mpesa_handlers.rs
use axum::{
    extract::{State, Json, Query},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use tracing::{info, error};

use crate::state::AppState;
use crate::services::mpesa_service::MpesaService;

// C2B Request
#[derive(Debug, Deserialize)]
pub struct StkPushRequest {
    pub phone_number: String,
    pub amount: String,
    pub account_reference: Option<String>,
    pub transaction_desc: Option<String>,
}

// C2B Response
#[derive(Debug, Serialize)]
pub struct StkPushResponse {
    pub success: bool,
    pub merchant_request_id: String,
    pub checkout_request_id: String,
    pub response_code: String,
    pub response_description: String,
    pub customer_message: String,
}

// Status Query
#[derive(Debug, Deserialize)]
pub struct StatusQuery {
    pub checkout_request_id: Option<String>,
    pub merchant_request_id: Option<String>,
}

// Callback Request
#[derive(Debug, Deserialize)]
pub struct MpesaCallback {
    pub Body: CallbackBody,
}

#[derive(Debug, Deserialize)]
pub struct CallbackBody {
    #[serde(rename = "stkCallback")]
    pub stk_callback: StkCallback,
}

#[derive(Debug, Deserialize)]
pub struct StkCallback {
    #[serde(rename = "MerchantRequestID")]
    pub merchant_request_id: String,

    #[serde(rename = "CheckoutRequestID")]
    pub checkout_request_id: String,

    #[serde(rename = "ResultCode")]
    pub result_code: i32,

    #[serde(rename = "ResultDesc")]
    pub result_desc: String,

    #[serde(rename = "CallbackMetadata")]
    pub callback_metadata: Option<CallbackMetadata>,
}

#[derive(Debug, Deserialize)]
pub struct CallbackMetadata {
    #[serde(rename = "Item")]
    pub items: Vec<CallbackItem>,
}

#[derive(Debug, Deserialize)]
pub struct CallbackItem {
    #[serde(rename = "Name")]
    pub name: String,

    #[serde(rename = "Value")]
    pub value: serde_json::Value,
}

// C2B Handlers
pub async fn initiate_stk_push(
    State(state): State<AppState>,
    Json(request): Json<StkPushRequest>,
) -> impl IntoResponse {
    info!("Received STK push request: {:?}", request);

    let mpesa_service = match &state.mpesa_service {
        Some(service) => service,
        None => {
            error!("M-Pesa service not available");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "success": false,
                    "error": "M-Pesa service is not available"
                }))
            );
        }
    };

    match mpesa_service.initiate_stk_push(
        &request.phone_number,
        &request.amount,
        request.account_reference.as_deref(),
        request.transaction_desc.as_deref(),
    ).await {
        Ok(response) => {
            info!("STK push initiated: {}", response.merchant_request_id);

            let api_response = StkPushResponse {
                success: true,
                merchant_request_id: response.merchant_request_id,
                checkout_request_id: response.checkout_request_id,
                response_code: response.response_code,
                response_description: response.response_description,
                customer_message: response.customer_message,
            };

            (StatusCode::OK, Json(serde_json::json!(api_response)))
        }
        Err(e) => {
            error!("Failed to initiate STK push: {}", e);

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "success": false,
                    "error": e.to_string()
                }))
            )
        }
    }
}

pub async fn mpesa_callback(
    Json(payload): Json<MpesaCallback>,
) -> impl IntoResponse {
    info!("Received M-Pesa callback: {:?}", payload.Body.stk_callback);

    // Always return success to M-Pesa
    Json(serde_json::json!({
        "ResultCode": 0,
        "ResultDesc": "Success"
    }))
}

pub async fn check_transaction_status(
    Query(query): Query<StatusQuery>,
) -> impl IntoResponse {
    info!("Checking transaction status: {:?}", query);

    Json(serde_json::json!({
        "status": "pending",
        "checkout_request_id": query.checkout_request_id,
        "merchant_request_id": query.merchant_request_id,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}

pub async fn get_transactions() -> impl IntoResponse {
    Json(serde_json::json!({
        "transactions": [],
        "count": 0
    }))
}

pub async fn get_stats() -> impl IntoResponse {
    Json(serde_json::json!({
        "total": 0,
        "successful": 0,
        "failed": 0
    }))
}

pub async fn simulate_payment(
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    info!("Simulating payment: {:?}", payload);

    Json(serde_json::json!({
        "success": true,
        "message": "Simulation successful",
        "merchant_request_id": "SIM-123456",
        "checkout_request_id": "ws_CO_SIM_123456",
    }))
}