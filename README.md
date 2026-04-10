# Chat Backend

Real-time chat backend built with Rust, Axum, SeaORM, and WebSockets.

## Quick Start

```bash
# 1. Copy and edit environment config
cp .env.example .env

# 2. Run database migrations
for f in migrations/*.up.sql; do
  psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f "$f"
done

# 3. Start the server
cargo run
```

Server runs on `http://127.0.0.1:8080` by default.

## Project Structure

```
src/
├── routes/       # HTTP handlers (auth, users, chats, ws)
├── service/      # Business logic
├── repository/   # Database queries
├── rooms/        # Realtime room management & presence
├── websocket/    # WebSocket connection & message dispatch
├── entities/     # SeaORM models
└── models/       # Request / response types
```

## API Endpoints

**Auth** – `POST /api/auth/register`, `POST /api/auth/login`

**Users** – `GET /api/users/me`, avatar upload & update

**Chats** – create, list, members, message history, search, image upload

**Read Receipts** – cursor `POST /api/chats/{chat_id}/read`

**WebSocket** – `GET /ws`

**Health** – `GET /health`, `GET /ready`

> All endpoints (except register/login) require `Authorization: Bearer <token>`.

## Development

```bash
cargo test          # run tests
cargo fmt --check   # check formatting
```

## Config

See [`.env.example`](.env.example) for all available environment variables.
