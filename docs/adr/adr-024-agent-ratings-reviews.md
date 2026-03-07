# ADR-024: Agent Rating & Review System

- **Status**: Accepted
- **Date**: 2026-03-07
- **Phase**: 7A

## Context

The AGNOS marketplace (mela) allows agents and desktop apps to be published,
discovered, and installed. To help users make informed decisions about which
packages to install, and to give publishers actionable feedback, the marketplace
needs a rating and review system.

## Decision

We introduce a **rating and review system** in the marketplace module
(`agent-runtime/src/marketplace/ratings.rs`) with the following design:

### Data Model

- **Rating**: A single review tied to `(agent_id, package_name)`. Contains a
  1-5 star score, an optional review text (max 2000 characters), a timestamp,
  and the version that was reviewed.
- **RatingStats**: Aggregate view per package -- average score, total count,
  score distribution histogram, and latest review timestamp.
- **RatingFilter**: Query criteria supporting min_score, package_name,
  agent_id, and date range filtering.

### Deduplication

One rating per agent per package. If the same agent submits a new rating for
the same package, the previous rating is replaced. This prevents ballot-stuffing
while allowing users to update their opinion after a new version.

### Storage

Ratings are stored in-memory via `RatingStore` (a nested HashMap keyed by
package then agent). Persistence is JSON file-based via `save()`/`load()`,
consistent with the existing `local_registry` pattern. The store gracefully
returns an empty instance when the file does not exist.

### Input Validation

All inputs are validated at the `add_rating()` boundary:
- `agent_id` and `package_name` must be non-empty
- `score` must be 1-5
- Review text must not exceed 2000 characters

### Queries

- `get_ratings(filter)` -- returns matching ratings sorted newest-first
- `get_stats(package)` -- returns aggregate statistics
- `top_rated(min_ratings)` -- returns packages sorted by average rating,
  with an optional minimum-ratings threshold to exclude low-sample packages

## Consequences

- Users and agents can discover high-quality packages via `top_rated()`
- Publishers get quantitative and qualitative feedback
- The deduplication policy keeps the system fair without requiring complex
  anti-fraud mechanisms at this stage
- JSON persistence is simple but sufficient for single-node alpha; a database
  backend can replace `save()`/`load()` post-alpha without changing the API
- Future work: HTTP API endpoints for ratings (Phase 7B), abuse/spam detection,
  verified-install badges
