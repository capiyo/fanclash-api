// config.rs
use std::env;

#[derive(Debug, Clone)]
pub struct AppConfig {
    // Database
    pub database_url: String,
    pub mongodb_url: String,

    // JWT
    pub jwt_secret: String,
    pub secret_key: String,

    // M-Pesa
    pub mpesa_consumer_key: String,
    pub mpesa_consumer_secret: String,
    pub mpesa_short_code: String,
    pub mpesa_passkey: String,
    pub mpesa_environment: String,
    pub mpesa_callback_url: String,
    pub mpesa_b2c_result_url: String,
    pub mpesa_b2c_queue_timeout_url: String,
    pub mpesa_initiator_name: String,
    pub mpesa_security_credential: String,

    // Server
    pub port: u16,
    pub host: String,
}

impl AppConfig {
    pub fn from_env() -> Self {
        AppConfig {
            // Database
            database_url: env::var("DATABASE_URL")
                .expect("DATABASE_URL must be set"),
            mongodb_url: env::var("MONGODB_URL")
                .expect("MONGODB_URL must be set"),

            // JWT
            jwt_secret: env::var("JWT_SECRET")
                .expect("JWT_SECRET must be set"),
            secret_key: env::var("SECRET_KEY")
                .expect("SECRET_KEY must be set"),

            // M-Pesa
            mpesa_consumer_key: env::var("MPESA_CONSUMER_KEY")
                .expect("MPESA_CONSUMER_KEY must be set"),
            mpesa_consumer_secret: env::var("MPESA_CONSUMER_SECRET")
                .expect("MPESA_CONSUMER_SECRET must be set"),
            mpesa_short_code: env::var("MPESA_SHORT_CODE")
                .expect("MPESA_SHORT_CODE must be set"),
            mpesa_passkey: env::var("MPESA_PASSKEY")
                .expect("MPESA_PASSKEY must be set"),
            mpesa_environment: env::var("MPESA_ENVIRONMENT")
                .unwrap_or_else(|_| "production".to_string()),
            mpesa_callback_url: env::var("MPESA_CALLBACK_URL")
                .expect("MPESA_CALLBACK_URL must be set"),
            mpesa_b2c_result_url: env::var("MPESA_B2C_RESULT_URL")
                .unwrap_or_else(|_| env::var("MPESA_CALLBACK_URL").unwrap() + "/b2c/result"),
            mpesa_b2c_queue_timeout_url: env::var("MPESA_B2C_QUEUE_TIMEOUT_URL")
                .unwrap_or_else(|_| env::var("MPESA_CALLBACK_URL").unwrap() + "/b2c/timeout"),
            mpesa_initiator_name: env::var("MPESA_INITIATOR_NAME")
                .unwrap_or_else(|_| "testapi".to_string()),
            mpesa_security_credential: env::var("MPESA_SECURITY_CREDENTIAL")
                .unwrap_or_else(|_| "".to_string()),

            // Server
            port: env::var("PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .expect("PORT must be a number"),
            host: env::var("HOST")
                .unwrap_or_else(|_| "0.0.0.0".to_string()),
        }
    }

    pub fn get_mpesa_urls(&self) -> (String, String, String) {
        if self.mpesa_environment == "production" {
            (
                "https://api.safaricom.co.ke/oauth/v1/generate?grant_type=client_credentials".to_string(),
                "https://api.safaricom.co.ke/mpesa/stkpush/v1/processrequest".to_string(),
                "https://api.safaricom.co.ke/mpesa/b2c/v1/paymentrequest".to_string()
            )
        } else {
            (
                "https://sandbox.safaricom.co.ke/oauth/v1/generate?grant_type=client_credentials".to_string(),
                "https://sandbox.safaricom.co.ke/mpesa/stkpush/v1/processrequest".to_string(),
                "https://sandbox.safaricom.co.ke/mpesa/b2c/v1/paymentrequest".to_string()
            )
        }
    }
}