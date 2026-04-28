![Rust](https://img.shields.io/badge/Rust-1.85+-orange?logo=rust)
![Axum](https://img.shields.io/badge/Axum-0.8-blue)
![MongoDB](https://img.shields.io/badge/MongoDB-Atlas-green?logo=mongodb)
![License](https://img.shields.io/badge/License-MIT-lightgrey)

# Insighta Labs+ — Backend API

REST API powering the Insighta Labs+ profile intelligence platform, built with Rust and Axum. It classifies names into demographic profiles by calling three external APIs (Genderize, Agify, Nationalize) and persisting results in MongoDB Atlas. All endpoints are secured with GitHub OAuth + PKCE, serving two client types from a single backend: a CLI (Bearer token) and a web portal (HTTP-only cookies).

---

## Stack

| Layer         | Tool                         | Purpose                                  |
| ------------- | ---------------------------- | ---------------------------------------- |
| Runtime       | Rust 1.85+ (edition 2024)    | —                                        |
| Web           | Axum 0.8                     | Routing, middleware, extractors          |
| Database      | MongoDB Atlas                | Profiles, users, refresh tokens          |
| Auth          | jsonwebtoken + tower-cookies | JWT signing, HTTP-only cookie management |
| HTTP client   | Reqwest                      | External API calls + GitHub OAuth        |
| Observability | tracing + tracing-subscriber | Structured JSON logs                     |

---

## Setup

### GitHub OAuth App

1. Go to **GitHub → Settings → Developer Settings → GitHub Apps → New GitHub App**.
2. Set the following fields:
   - **Application name**: Insighta Labs (or any name)
   - **Homepage URL**: your deployed app URL, or `http://localhost:8000` for local dev
   - **Authorization callback URL**: `http://localhost:5173/callback` for local web dev; add your production web URL for deployment
3. Click **Register application**, then copy the **Client ID**.
4. Click **Generate a new client secret** and copy it immediately — it is not shown again.
5. Set `GITHUB_CLIENT_ID` and `GITHUB_CLIENT_SECRET` in your `.env`.

GitHub Apps support multiple callback URLs, so the web and CLI callbacks can all be registered on the same app.

### Admin ID Setup

Admin role is granted by GitHub **numeric user ID**, not by username. To find a GitHub user's numeric ID:

```bash
curl https://api.github.com/users/<github-username> | grep '"id"'
# → "id": 12345678
```

Set one or more IDs in `.env`:

```env
ADMIN_GITHUB_IDS=12345678
ADMIN_GITHUB_IDS=12345678,87654321   # multiple admins
```

Role assignment is evaluated on **every login** — adding or removing an ID takes effect the next time that user signs in, with no server restart required.

---

## Quick Start

```bash
git clone <repo> && cd insighta-api
cp .env.example .env   # fill in values
cargo run
# → Server on http://0.0.0.0:8000
```

---

## Environment Variables

| Variable               | Required | Default                     | Description                                       |
| ---------------------- | :------: | --------------------------- | ------------------------------------------------- |
| `DATABASE_URL`         |          | `mongodb://localhost:27017` | MongoDB connection string                         |
| `DATABASE_NAME`        |          | `stage2`                    | Database name                                     |
| `GITHUB_CLIENT_ID`     |    ✅    | —                           | GitHub OAuth App client ID                        |
| `GITHUB_CLIENT_SECRET` |    ✅    | —                           | GitHub OAuth App client secret                    |
| `GITHUB_REDIRECT_URI`  |          | —                           | OAuth redirect URI (required for CLI flow)        |
| `JWT_SECRET`           |    ✅    | —                           | HMAC secret for signing access tokens             |
| `ADMIN_GITHUB_IDS`     |          | `""`                        | Comma-separated GitHub numeric user IDs for admin |
| `PORT`                 |          | `8000`                      | Server bind port                                  |
| `SECURE_COOKIES`       |          | `false`                     | Set `true` in production (HTTPS-only cookies)     |

> **Note:** `ADMIN_GITHUB_IDS` uses GitHub's numeric user IDs, not usernames. Role assignment is evaluated on every login — changes take effect on the next sign-in without a server restart.

---

## Docker

```bash
docker build -t insighta-api .
docker run -p 8000:8000 --env-file .env insighta-api
```

---

## Authentication

All auth uses GitHub OAuth 2.0 with PKCE (RFC 7636). No passwords are stored. Two client types share the same flow with different token delivery mechanisms.

### CLI Flow

**Verifier generation**

The CLI derives all parameters locally before the browser opens:

```
code_verifier  →  32 random bytes, hex-encoded (64 chars)
code_challenge →  BASE64URL-NOPAD( SHA-256(code_verifier) )
state          →  16 random bytes, hex-encoded (CSRF token)
```

**GitHub redirect**

```
GET /auth/github
  ?state=<state>
  &code_challenge=<challenge>
  &redirect_uri=http://localhost:8182/callback
```

The backend stores `state → (challenge, redirect_uri, created_at)` in memory and issues a redirect to GitHub. A background task prunes entries older than 5 minutes every 60 seconds to prevent memory leaks from abandoned flows.

**Callback capture**

GitHub redirects to the CLI's local TCP server. The CLI validates `returned_state == state`, then calls:

```
GET /auth/github/callback
  ?code=<code>
  &state=<state>
  &code_verifier=<verifier>
```

**Token exchange**

The backend verifies `BASE64URL(SHA-256(verifier)) == stored_challenge`, exchanges the code with GitHub, upserts the user record, and returns:

```json
{ "status": "success", "access_token": "...", "refresh_token": "..." }
```

The CLI decodes the username from the JWT payload and saves credentials to `~/.insighta/credentials.json`.

> The authorization window is 3 minutes. If the local server receives no callback, the flow times out and the user must re-run `insighta login`.

---

### Web Flow

**Verifier generation**

The browser generates the PKCE pair using `crypto.subtle` and stores the verifier in `sessionStorage`:

```
code_verifier  →  32 random bytes, hex-encoded
code_challenge →  BASE64URL-NOPAD( SHA-256(code_verifier) )  [via crypto.subtle.digest]
state          →  crypto.randomUUID(), hyphens stripped
```

**GitHub redirect**

```
GET /auth/github
  ?state=<state>
  &code_challenge=<challenge>
  &redirect_uri=<web-origin>/callback
```

**Callback validation**

The `/callback` page checks `state` against `sessionStorage`, retrieves `code_verifier`, then calls:

```
POST /auth/web/exchange?code=<code>&state=<state>&code_verifier=<verifier>
```

**Cookie issuance**

The backend validates PKCE and sets three cookies:

| Cookie          | TTL   | `HttpOnly` |   JS-readable    | Purpose            |
| --------------- | ----- | :--------: | :--------------: | ------------------ |
| `access_token`  | 3 min |     ✅     |        ❌        | API authentication |
| `refresh_token` | 5 min |     ✅     |        ❌        | Token rotation     |
| `csrf_token`    | 5 min |     ❌     | ✅ (intentional) | CSRF double-submit |

> `access_token` and `refresh_token` are never readable by JavaScript. The `csrf_token` is intentionally readable so the frontend can attach it as `X-CSRF-Token` on mutating requests.

---

## Token Handling

**Access token**

| Property  | Value                                                               |
| --------- | ------------------------------------------------------------------- |
| Format    | JWT, signed HS256                                                   |
| Expiry    | 3 minutes                                                           |
| Claims    | `sub` (UUID), `role`, `username`, `iat`, `exp`                      |
| Transport | `Authorization: Bearer <token>` (CLI) / `access_token` cookie (web) |

**Refresh token**

| Property    | Value                                                                |
| ----------- | -------------------------------------------------------------------- |
| Format      | 64-char opaque hex string                                            |
| Expiry      | 5 minutes                                                            |
| Storage     | SHA-256 hash persisted to MongoDB; raw value is never stored         |
| Auto-expiry | MongoDB TTL index on `expires_at` auto-deletes stale documents       |
| Consumption | `find_one_and_delete` — one-time use, atomically invalidated on read |

Each refresh issues a completely new token pair. The old token is invalidated in the same atomic operation that produces the new one. The CLI retries the original request automatically after a successful refresh; the web portal redirects to login if the refresh fails.

---

## Role Enforcement

### Roles

| Role      | Assigned when                             | Permissions                          |
| --------- | ----------------------------------------- | ------------------------------------ |
| `admin`   | GitHub ID present in `ADMIN_GITHUB_IDS`   | Create, delete, read, search, export |
| `analyst` | Default for all other authenticated users | Read, search, export                 |

### Enforcement Chain

1. `require_auth` middleware validates the Bearer token or cookie, loads the user record, and injects `AuthenticatedUser` into request extensions.
2. `RequireAny` / `RequireAdmin` extractors on each route read from that extension.
3. Any user with `is_active = false` receives `403 Forbidden` regardless of role.

---

## API Reference

> **All `/api/*` requests require the `X-API-Version: 1` header.** Missing or unrecognised values return `400`.

### Auth Endpoints

| Method | Path                    | Auth   | Description                                        |
| ------ | ----------------------- | ------ | -------------------------------------------------- |
| GET    | `/auth/github`          | —      | Redirect to GitHub OAuth (CLI + web)               |
| GET    | `/auth/github/callback` | —      | Exchange code, issue access + refresh tokens (CLI) |
| POST   | `/auth/refresh`         | —      | Rotate refresh token pair                          |
| POST   | `/auth/logout`          | —      | Invalidate refresh token                           |
| POST   | `/auth/web/exchange`    | —      | Exchange code, set HTTP-only cookies (web)         |
| POST   | `/auth/web/refresh`     | CSRF   | Rotate cookie pair                                 |
| POST   | `/auth/web/logout`      | CSRF   | Clear cookies                                      |
| GET    | `/auth/me`              | Cookie | Return current user info                           |

### Profile Endpoints

| Method | Path                   | Role  | Description                            |
| ------ | ---------------------- | ----- | -------------------------------------- |
| GET    | `/api/profiles`        | Any   | List with filters, sorting, pagination |
| POST   | `/api/profiles`        | Admin | Create profile (calls 3 external APIs) |
| GET    | `/api/profiles/{id}`   | Any   | Single profile by UUID                 |
| DELETE | `/api/profiles/{id}`   | Admin | Delete profile (`204 No Content`)      |
| GET    | `/api/profiles/search` | Any   | Natural language search (`?q=`)        |
| GET    | `/api/profiles/export` | Any   | Download CSV (`?format=csv`)           |

**`GET /api/profiles` — query parameters:**

| Parameter                 | Type    | Default | Description                                  |
| ------------------------- | ------- | ------- | -------------------------------------------- |
| `gender`                  | string  | —       | `male` or `female`                           |
| `age_group`               | string  | —       | `child`, `teenager`, `adult`, or `senior`    |
| `country_id`              | string  | —       | ISO 3166-1 alpha-2 (e.g. `NG`)               |
| `min_age` / `max_age`     | integer | —       | Inclusive age bounds                         |
| `min_gender_probability`  | float   | —       | Gender confidence threshold (0.0–1.0)        |
| `min_country_probability` | float   | —       | Country confidence threshold (0.0–1.0)       |
| `sort_by`                 | string  | `age`   | `age`, `created_at`, or `gender_probability` |
| `order`                   | string  | `asc`   | `asc` or `desc`                              |
| `page`                    | integer | `1`     | Page number (1-indexed)                      |
| `limit`                   | integer | `10`    | Results per page (max 50)                    |

### Pagination

```json
{
  "status": "success",
  "page": 1,
  "limit": 10,
  "total": 2026,
  "total_pages": 203,
  "links": {
    "self": "/api/profiles?page=1&limit=10",
    "next": "/api/profiles?page=2&limit=10",
    "prev": null
  },
  "data": [...]
}
```

### Error Format

All errors use a consistent envelope:

```json
{ "status": "error", "message": "Detailed error message here" }
```

---

## Natural Language Search

`GET /api/profiles/search?q=<query>` runs a rule-based, single-pass token scanner — no AI. The parser lowercases the input, splits on whitespace, and scans left-to-right to produce structured filters. Explicit query parameters (e.g. `&gender=male`) always override values inferred from the text. Returns `400` if no filter could be parsed.

<details>
<summary>Keyword reference</summary>

**Gender**

| Tokens                                                                   | Filter          |
| ------------------------------------------------------------------------ | --------------- |
| `male`, `males`, `man`, `men`, `boy`, `boys`                             | `gender=male`   |
| `female`, `females`, `woman`, `women`, `girl`, `girls`, `lady`, `ladies` | `gender=female` |

> If both gender token groups appear in the same query, the gender filter is dropped entirely.

---

**Age groups**

| Tokens                                                  | Filter                   |
| ------------------------------------------------------- | ------------------------ |
| `child`, `children`, `kid`, `kids`                      | `age_group=child`        |
| `teenager`, `teenagers`, `teen`, `teens`                | `age_group=teenager`     |
| `adult`, `adults`, `grownup`, `grownups`, `middle-aged` | `age_group=adult`        |
| `senior`, `seniors`, `old`, `elderly`                   | `age_group=senior`       |
| `young`                                                 | `min_age=16, max_age=24` |

---

**Age range bigrams** (`keyword N`)

| Pattern                        | Filter      |
| ------------------------------ | ----------- |
| `above N`, `over N`, `least N` | `min_age=N` |
| `below N`, `under N`, `most N` | `max_age=N` |

---

**Country**

`from [country]` or `in [country]` resolves to a `country_id` (ISO 3166-1 α-2). The parser scans up to 7 tokens for multi-word country names. A query that is itself a country name also works (e.g. `nigeria`).

---

**Sort / limit bigrams**

| Pattern                          | Effect                          |
| -------------------------------- | ------------------------------- |
| `top N`, `first N`, `latest N`   | sort `created_at` desc, limit N |
| `last N`, `oldest N`, `bottom N` | sort `created_at` asc, limit N  |

---

**Example queries**

| Query                    | Filters applied                                |
| ------------------------ | ---------------------------------------------- |
| `young males`            | `gender=male, min_age=16, max_age=24`          |
| `females above 30`       | `gender=female, min_age=30`                    |
| `adult males from kenya` | `gender=male, age_group=adult, country_id=KE`  |
| `top 5 women`            | `gender=female, sort created_at desc, limit 5` |
| `nigeria`                | `country_id=NG`                                |

</details>

---

## Rate Limiting

| Scope     | Limit      | Key                    |
| --------- | ---------- | ---------------------- |
| `/auth/*` | 10 req/min | IP (`X-Forwarded-For`) |
| `/api/*`  | 60 req/min | Authenticated user ID  |

Returns `429 Too Many Requests` when the limit is exceeded.

---

## CSRF Protection

Web SPA mutating requests use a **double-submit cookie pattern**:

1. On login, the backend sets a readable `csrf_token` cookie alongside the HTTP-only auth cookies (5-minute TTL).
2. The frontend reads `csrf_token` from `document.cookie` and sends it as the `X-CSRF-Token` request header.
3. The backend validates the header value against the cookie before processing `POST /auth/web/refresh` and `POST /auth/web/logout`.
4. `GET`, `HEAD`, and `OPTIONS` requests bypass the check entirely.

> Because `csrf_token` is tied to a specific session and unreadable cross-origin (same-origin policy), forged requests from other origins cannot supply a valid token.

---

## API Versioning

All `/api/*` requests must include:

```
X-API-Version: 1
```

Missing or unrecognised headers return:

```json
{ "status": "error", "message": "API version header required" }
```

---

## Database Schema

### `users` collection

| Field           | Type     | Notes                             |
| --------------- | -------- | --------------------------------- |
| `id`            | UUID v7  | Primary key                       |
| `github_id`     | integer  | Unique; GitHub's numeric user ID  |
| `username`      | string   | GitHub login handle               |
| `email`         | string   | May be `null` if hidden on GitHub |
| `avatar_url`    | string   | GitHub avatar URL                 |
| `role`          | string   | `admin` or `analyst`              |
| `is_active`     | bool     | `false` disables all access       |
| `last_login_at` | datetime | Updated on every successful login |
| `created_at`    | datetime | Set at first upsert               |

Indexes: unique on `id`, unique on `github_id`.

### `refresh_tokens` collection

| Field        | Type     | Notes                                               |
| ------------ | -------- | --------------------------------------------------- |
| `token_hash` | string   | SHA-256 of the raw token; raw value is never stored |
| `user_id`    | UUID v7  | Foreign key to `users.id`                           |
| `expires_at` | datetime | MongoDB TTL index; document auto-deleted on expiry  |

Indexes: unique on `token_hash`, TTL on `expires_at`.

### `profiles` collection

| Field                 | Type     | Notes                                     |
| --------------------- | -------- | ----------------------------------------- |
| `id`                  | UUID v7  | Primary key                               |
| `name`                | string   | Unique                                    |
| `gender`              | string   | `male` or `female`                        |
| `gender_probability`  | float    | Rounded to 2 decimal places               |
| `age`                 | integer  | —                                         |
| `age_group`           | string   | `child`, `teenager`, `adult`, or `senior` |
| `country_id`          | string   | ISO 3166-1 alpha-2                        |
| `country_name`        | string   | Full country name                         |
| `country_probability` | float    | Rounded to 2 decimal places               |
| `created_at`          | datetime | UTC                                       |

Age group classification: 0–12 → `child`, 13–19 → `teenager`, 20–59 → `adult`, 60+ → `senior`.

Indexes: unique on `id`, unique on `name`, compound on `(country_id, gender, age_group)`, single on `age`, `created_at`, `gender_probability`, `country_probability`.

---

## CLI Companion

The `insighta-cli` is a separate companion tool at [insighta-labs/insighta-cli](https://github.com/insighta-labs/insighta-cli). It communicates with this API using Bearer token auth and stores credentials at `~/.insighta/credentials.json`.

**Quick Install:**

```bash
chmod +x install.sh && ./install.sh
```

---

## Contributing & License

Contributions are welcome via pull request. Please open an issue first for significant changes.

Licensed under the [MIT License](LICENSE).
