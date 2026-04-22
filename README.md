# HNG Stage 2: Profile Intelligence API

A name classification and demographic intelligence service built with Rust. Integrates three external APIs (Genderize, Agify, Nationalize), persists results in MongoDB, and exposes a filterable, sortable, paginated REST API with a natural language search endpoint.

---

## Stack

- **Runtime**: Rust (Tokio async runtime)
- **Web framework**: Axum
- **Database**: MongoDB Atlas
- **HTTP client**: Reqwest
- **Serialization**: Serde / serde_json
- **Observability**: tracing + tracing-subscriber (structured JSON logs)

---

## Requirements

- Rust 1.85+ (edition 2024)
- Cargo
- MongoDB Atlas cluster (or local `mongod`)

---

## Local Setup

1. Clone the repository and install dependencies:

   ```bash
   cargo build
   ```

2. Create a `.env` file in the project root:

   ```env
   DATABASE_URL=mongodb+srv://<user>:<password>@<cluster>.mongodb.net/?retryWrites=true&w=majority
   DATABASE_NAME=test
   ```

3. Run the server:

   ```bash
   cargo run
   ```

   The server binds to `0.0.0.0:8000`.

---

## Data Seeding

On startup, the application runs a non-blocking background seeder. It reads from `seed_profiles.json` and inserts any missing records into MongoDB idempotently. If the database is already fully seeded, it skips the operation entirely.

---

## Docker

```bash
docker build -t profile-api .
docker run -e DATABASE_URL=<uri> -p 8000:8000 profile-api
```

---

## API Reference

All responses use `Content-Type: application/json`. All timestamps are UTC ISO 8601. All IDs are UUID v7. CORS is open (`Access-Control-Allow-Origin: *`).

### Error envelope

```json
{ "status": "error", "message": "<description>" }
```

| Status | Meaning                             |
| ------ | ----------------------------------- |
| 400    | Missing or empty parameter          |
| 404    | Profile not found                   |
| 422    | Invalid parameter type              |
| 502    | External API returned unusable data |

---

### POST /api/profiles

Create a profile by name. Calls Genderize, Agify, and Nationalize in parallel, classifies the result, and stores it.

**Request body**

```json
{ "name": "ella" }
```

**201 Created** — new profile:

```json
{
  "status": "success",
  "data": {
    "id": "019571b2-...",
    "name": "ella",
    "gender": "female",
    "gender_probability": 0.99,
    "age": 46,
    "age_group": "adult",
    "country_id": "DK",
    "country_name": "Denmark",
    "country_probability": 0.21,
    "created_at": "2026-04-20T10:00:00Z"
  }
}
```

**200 OK** — name already exists (idempotent):

```json
{
  "status": "success",
  "message": "Profile already exists",
  "data": { "...existing profile..." }
}
```

**Edge cases that return 502 and do not store:**

- Genderize returns `gender: null` or `count: 0`
- Agify returns `age: null`
- Nationalize returns an empty country array

---

### GET /api/profiles/{id}

Fetch a single profile by UUID.

**200 OK** — returns full profile object as shown above.
**404** — profile not found.

---

### GET /api/profiles

List profiles with optional filtering, sorting, and pagination.

**Query parameters**

| Parameter                 | Type    | Description                                                   |
| ------------------------- | ------- | ------------------------------------------------------------- |
| `gender`                  | string  | `male` or `female` (case-insensitive)                         |
| `age_group`               | string  | `child`, `teenager`, `adult`, `senior` (case-insensitive)     |
| `country_id`              | string  | ISO 3166-1 alpha-2 code, e.g. `NG` (case-insensitive)         |
| `min_age`                 | integer | Minimum age (inclusive)                                       |
| `max_age`                 | integer | Maximum age (inclusive)                                       |
| `min_gender_probability`  | float   | Minimum gender confidence score                               |
| `min_country_probability` | float   | Minimum country confidence score                              |
| `sort_by`                 | string  | `age`, `created_at`, or `gender_probability` (default: `age`) |
| `order`                   | string  | `asc` or `desc` (default: `asc`)                              |
| `page`                    | integer | Page number, 1-indexed (default: `1`)                         |
| `limit`                   | integer | Results per page, max 50 (default: `10`)                      |

All filters are combinable. Every condition must match.

**Example**

```
GET /api/profiles?gender=male&country_id=NG&min_age=25&sort_by=age&order=desc&page=1&limit=10
```

**200 OK**

```json
{
  "status": "success",
  "page": 1,
  "limit": 10,
  "total": 312,
  "data": [
    {
      "id": "...",
      "name": "emmanuel",
      "gender": "male",
      "gender_probability": 0.99,
      "age": 34,
      "age_group": "adult",
      "country_id": "NG",
      "country_name": "Nigeria",
      "country_probability": 0.85,
      "created_at": "2026-04-01T12:00:00Z"
    }
  ]
}
```

---

### GET /api/profiles/search?q=

Natural language search. Parses a plain English query into structured filters and runs the same paginated query as `GET /api/profiles`.

Accepts the same `page`, `limit`, `sort_by`, and `order` query parameters. Explicit query parameters override any values inferred from the natural language text.

**Example**

```
GET /api/profiles/search?q=young males from nigeria&page=1&limit=20
```

**400 Bad Request** — if the query cannot be interpreted:

```json
{ "status": "error", "message": "Unable to interpret query" }
```

See the [Natural Language Parsing](#natural-language-parsing) section for full keyword documentation.

---

### DELETE /api/profiles/{id}

Delete a profile by UUID. Returns **204 No Content** on success, **404** if not found.

---

## Natural Language Parsing

The search endpoint uses a rule-based, single-pass token scanner. There is no AI or LLM involved. The parser lowercases the query, splits on whitespace, and scans the resulting tokens left to right. Each recognized token or bigram (two consecutive tokens) sets one or more filters. Unrecognized tokens are ignored. If no token produces a filter match, the endpoint returns a 400 with `"Unable to interpret query"`.

### Gender keywords

| Keywords                                                                 | Filter set      |
| ------------------------------------------------------------------------ | --------------- |
| `male`, `males`, `man`, `men`, `boy`, `boys`                             | `gender=male`   |
| `female`, `females`, `woman`, `women`, `girl`, `girls`, `lady`, `ladies` | `gender=female` |

If both male and female keywords appear in the same query, the gender filter is dropped and results are not filtered by gender.

### Age group keywords

| Keywords                                                | Filter set                 |
| ------------------------------------------------------- | -------------------------- |
| `child`, `children`, `kid`, `kids`                      | `age_group=child`          |
| `teenager`, `teenagers`, `teen`, `teens`                | `age_group=teenager`       |
| `adult`, `adults`, `grownup`, `grownups`, `middle-aged` | `age_group=adult`          |
| `senior`, `seniors`, `old`, `elderly`                   | `age_group=senior`         |
| `young`                                                 | `min_age=16`, `max_age=24` |

`young` is a special case. It maps to an age range (16–24) rather than a stored age group, and is used for query purposes only. It is not a value that appears in the database.

### Age range keywords

These work as bigrams — the keyword must be followed immediately by a number (either digits or written out, e.g. "20" or "twenty").

| Pattern                        | Filter set  |
| ------------------------------ | ----------- |
| `above N`, `over N`, `least N` | `min_age=N` |
| `below N`, `under N`, `most N` | `max_age=N` |

`least` and `most` are designed to match the trailing word from "at least" and "at most" — the leading "at" is a stop word and is ignored.

### Country keywords

| Pattern                          | Filter set              |
| -------------------------------- | ----------------------- |
| `from [country]`, `in [country]` | `country_id=<ISO code>` |
| Entire query is a country name   | `country_id=<ISO code>` |

Country names are matched against a static ISO 3166-1 lookup table of ~250 entries in a fully case-insensitive manner. The parser scans up to 7 tokens ahead after `from`/`in` to robustly resolve multi-word countries like "United States of America" or "Bosnia and Herzegovina".

### Sort and limit keywords

These also work as bigrams — the keyword must be followed immediately by a number (either digits or written out).

| Pattern                          | Behaviour                                           |
| -------------------------------- | --------------------------------------------------- |
| `top N`, `first N`, `latest N`   | Sort by `created_at` descending, limit to N results |
| `last N`, `oldest N`, `bottom N` | Sort by `created_at` ascending, limit to N results  |

### Example mappings

| Query                                | Filters applied                                    |
| ------------------------------------ | -------------------------------------------------- |
| `young males`                        | `gender=male`, `min_age=16`, `max_age=24`          |
| `females above 30`                   | `gender=female`, `min_age=30`                      |
| `people from angola`                 | `country_id=AO`                                    |
| `adult males from kenya`             | `gender=male`, `age_group=adult`, `country_id=KE`  |
| `male and female teenagers above 17` | `age_group=teenager`, `min_age=17`                 |
| `top 5 women`                        | `gender=female`, sort `created_at` desc, limit 5   |
| `elderly men in japan`               | `gender=male`, `age_group=senior`, `country_id=JP` |
| `nigeria`                            | `country_id=NG`                                    |

### Stop words

Common conversational words (`people`, `person`, `show`, `find`, `give`, `me`, `who`, `are`, `is`, `list`, `all`, `everyone`, `anybody`, `someone`, `with`, `the`, `a`, `an`, `of`, `that`, `have`, `profiles`, `records`, `entries`) are formally identified and safely ignored. Because of this, a purely conversational query containing no demographic filters, such as "show me all people", will correctly return a 400 error (`"Unable to interpret query"`) because no meaningful constraints were parsed.

---

## Limitations

The parser is intentionally rule-based and covers the most common demographic query patterns. The following cases are explicitly not handled.

**Country matching requires prepositions.** Unless the country name is the _entire_ query (e.g., `q=nigeria`), the parser will only recognize a country if it is immediately preceded by `in` or `from`. A query like "young males in japan" works perfectly, but "young males japan" will fail to parse the country.

**Strict phrasing and bigram ordering.** Age ranges and limit modifiers strictly require the recognized keyword to immediately precede the number. "above 30" works, but "30 and above", "30+", or "older than 30" will not trigger the filter.

**Negation.** "not male", "non-adults", "excluding nigeria" have no effect. The parser has no concept of exclusion.

**Probability filters.** There is no natural language support for "high confidence" or "probability above 0.9". Use the `min_gender_probability` and `min_country_probability` query parameters directly.

**Compound country queries.** "from nigeria or kenya" is not supported. Only the first successfully matched country filter is applied.

**Conflicting age constraints.** If the query contains both an age group keyword and an explicit age range that falls outside that group (e.g. "adults above 70"), both filters are applied independently to the database query. The result set will be empty because no profile can satisfy `age_group=adult` and `age >= 70` simultaneously. The parser does not validate for semantic conflicts between filters.

**`young` combined with an explicit age range.** "young males above 20" sets `min_age=16` from "young" and then `min_age=20` from "above 20". The later assignment wins. The word "young" does not constrain `max_age` in this case unless no other age keyword is present.

---

## Database Schema

| Field                 | Type            | Notes                                     |
| --------------------- | --------------- | ----------------------------------------- |
| `id`                  | UUID v7         | Primary key                               |
| `name`                | string (unique) | Person's name                             |
| `gender`              | string          | `male` or `female`                        |
| `gender_probability`  | float           | Rounded to 2 decimal places               |
| `age`                 | integer         |                                           |
| `age_group`           | string          | `child`, `teenager`, `adult`, or `senior` |
| `country_id`          | string          | ISO 3166-1 alpha-2 code                   |
| `country_name`        | string          | Full country name                         |
| `country_probability` | float           | Rounded to 2 decimal places               |
| `created_at`          | string          | UTC ISO 8601                              |

**Age group classification:**

| Range | Group    |
| ----- | -------- |
| 0–12  | child    |
| 13–19 | teenager |
| 20–59 | adult    |
| 60+   | senior   |

**Indexes:**

- Unique on `id`
- Unique on `name`
- Compound on `(country_id, gender, age_group)` for filter queries
- Single on `age` for range queries
- Single on `created_at` for sort
- Single on `gender_probability` for sort and probability filter
- Single on `country_probability` for probability filter

---
