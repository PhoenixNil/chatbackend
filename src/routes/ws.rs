use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use serde::Deserialize;

use crate::errors::AppError;
use crate::state::SharedAppState;

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    pub token: String,
}

pub async fn upgrade(
    ws: WebSocketUpgrade,
    State(state): State<SharedAppState>,
    Query(query): Query<WsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = super::user_id_from_token(state.as_ref(), &query.token)?;

    Ok(ws.on_upgrade(move |socket| async move {
        crate::websocket::connection::handle_socket(state, user_id, socket).await;
    }))
}
