use std::sync::Arc;
use mongodb::Database;

// If we have a MpesaService struct in services::mpesa_service
use crate::services::mpesa_service::MpesaService;

#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub mpesa_service: Option<Arc<MpesaService>>,
}

impl AppState {
    pub fn new(db: Database) -> Self {
        AppState {
            db,
            mpesa_service: None,
        }
    }

    pub fn with_mpesa(mut self, mpesa_service: Arc<MpesaService>) -> Self {
        self.mpesa_service = Some(mpesa_service);
        self
    }
}