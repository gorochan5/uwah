# Webhooks, Outbox and Dead-Letter Queue

Webhook delivery is optional and configured in `uwah.toml`:

```toml
[webhook]
enabled = true
url = "https://example.test/uwah/events"
secret = "shared-secret"
```

When enabled, gateway event payloads are persisted in `outbox_events` and dispatched asynchronously by the background outbox worker. The worker uses retry scheduling and leases to avoid permanently claiming an event. Events that exhaust retries or are rejected permanently are moved to `webhook_dlq`.

## Signature

The request body is signed with HMAC-SHA256 using `secret`. The preferred `X-Uwah-Signature` format is:

```text
t=<unix_timestamp>,v1=<hex_digest>
```

The digest covers `<unix_timestamp>.<raw_request_body>`. The verifier also accepts the legacy `sha256=<hex_digest>` form, whose digest covers only the raw body.

## DLQ operations

All DLQ routes require `Authorization: Bearer <token>`:

| Method | Route | Purpose |
|---|---|---|
| GET | `/webhooks/dlq` | List quarantined events |
| GET | `/webhooks/dlq/{id}` | Inspect one event |
| POST | `/webhooks/dlq/{id}/replay` | Requeue one event |
| POST | `/webhooks/dlq/replay-all` | Requeue all events |
| DELETE | `/webhooks/dlq/{id}` | Permanently remove one event |

Webhook delivery is at-least-once: consumers must deduplicate by event ID. HMAC verification should use the raw request bytes before JSON parsing and should reject malformed signatures. The current verifier checks the signature but does not enforce a timestamp age window; replay protection beyond the signed timestamp must be implemented by the consumer.
